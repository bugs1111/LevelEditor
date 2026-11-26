use super::*;
use std::time::{Duration, Instant};

pub(super) fn options(hidden: &HashSet<i64>, selected: LevelKey) -> RenderOptions<'_> {
    RenderOptions {
        selected,
        active_layer: 0,
        world: false,
        world_multi_selection: false,
        world_hover: None,
        grid: true,
        details: true,
        single_layer: false,
        hidden,
        modified_levels: None,
        render_auto: true,
        tile_tags: false,
        ui_scale: 1.,
        single_layer_intensity: 0.75,
        fields_table: false,
        world_depth: None,
        nearby_dist: 64.,
        parallax: true,
        rulers: true,
        guides: true,
        async_paint: None,
    }
}
fn frame(
    ctx: &egui::Context,
    project: &Project,
    view: &Viewport,
    options: &RenderOptions<'_>,
    assets: &mut Assets,
) -> (usize, egui::FullOutput) {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 700.));
    let mut count = 0;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ctx| {
            assets.poll(ctx);
            let painter = ctx.layer_painter(egui::LayerId::background());
            count = draw(&painter, rect, project, view, options, None, assets);
        },
    );
    (count, output)
}

#[test]
#[ignore = "manual zoomed tile frame performance comparison"]
fn benchmark_zoomed_dense_tile_frame() {
    exercise_dense_tile_frames(false, [0.; 2]);
}

#[test]
#[ignore = "manual tile frame control with varying atlas coordinates and opacity"]
fn benchmark_varied_tile_styles_frame() {
    exercise_dense_tile_frames(true, [0.; 2]);
}

#[test]
#[ignore = "manual frame comparison with nonzero tile pivots"]
fn benchmark_shifted_tile_frame() {
    exercise_dense_tile_frames(true, [0.25, 0.75]);
}

fn exercise_dense_tile_frames(varied: bool, pivot: [f32; 2]) {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 160 * 16, 160 * 16, 0, 0).unwrap();
    project.add_layer("Ground", "Tiles", 16).unwrap();
    project.json["defs"]["layers"][0]["tilesetDefUid"] = json!(100);
    project.json["defs"]["layers"][0]["tilePivotX"] = json!(pivot[0]);
    project.json["defs"]["layers"][0]["tilePivotY"] = json!(pivot[1]);
    project.json["defs"]["tilesets"] = json!([{
        "uid":100,"relPath":null,"embedAtlas":"LdtkIcons","tileGridSize":16
    }]);
    project.layer_mut(key, 0).unwrap()["gridTiles"] = (0..160 * 160)
        .map(|i| {
            let mut tile = json!({"px":[i % 160 * 16,i / 160 * 16],"src":[0,0],"t":0,"f":i % 4});
            if varied {
                tile["src"] = json!([i % 16 * 16, (i / 16) % 4 * 16]);
                tile["a"] = json!([0.25, 0.75, 1.0][i as usize % 3]);
            }
            tile
        })
        .collect::<Vec<_>>()
        .into();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let texture = assets.embedded(&ctx).unwrap().id();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.grid = false;
    options.details = false;
    options.tile_tags = varied;
    let view = Viewport {
        center: [1280., 1280.],
        zoom: 4.,
    };
    let (expected_count, expected) = frame(&ctx, &project, &view, &options, &mut assets);
    let mesh = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.clone()),
                _ => None,
            })
            .unwrap()
    };
    let expected = mesh(&expected);
    assert_eq!(expected.vertices.len(), expected_count * 4);
    if pivot == [0.; 2] {
        assert_eq!(expected_count, 192);
        for (index, quad) in expected.vertices.as_chunks::<4>().0.iter().enumerate() {
            let min = egui::pos2(
                -12. + (index % 16) as f32 * 64.,
                -34. + (index / 16) as f32 * 64.,
            );
            assert_eq!(quad[0].pos, min);
            assert_eq!(quad[3].pos, min + egui::vec2(64., 64.));
        }
    }
    let mut hasher = DefaultHasher::new();
    format!("{expected:?}").hash(&mut hasher);
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let start = Instant::now();
        let (count, output) = frame(&ctx, &project, &view, &options, &mut assets);
        *time = start.elapsed();
        assert_eq!(count, expected_count);
        assert_eq!(mesh(&output), expected);
    }
    times.sort_unstable();
    eprintln!(
        "Zoomed dense tile frame: median {:?}; mesh {:016x}; cells {expected_count}; samples {times:?}",
        times[2],
        hasher.finish()
    );

    let view = Viewport {
        center: [1280., 1280.],
        zoom: 0.25,
    };
    let (count, expected) = frame(&ctx, &project, &view, &options, &mut assets);
    let expected = mesh(&expected);
    assert_eq!(count, 160 * 160);
    let mut hasher = DefaultHasher::new();
    format!("{expected:?}").hash(&mut hasher);
    for time in &mut times {
        let start = Instant::now();
        let (actual_count, output) = frame(&ctx, &project, &view, &options, &mut assets);
        *time = start.elapsed();
        assert_eq!(actual_count, count);
        assert_eq!(mesh(&output), expected);
    }
    times.sort_unstable();
    eprintln!(
        "Full dense tile frame: median {:?}; mesh {:016x}; samples {times:?}",
        times[2],
        hasher.finish()
    );
}

#[test]
#[ignore = "manual cropped repeating background performance comparison"]
fn benchmark_cropped_repeating_background() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 4096, 4096, 0, 0).unwrap();
    let level = project.level_mut(key).unwrap();
    level["bgRelPath"] = json!("background.png");
    level["bgPos"] = json!("Repeat");
    level["bgPivotX"] = json!(0.31);
    level["bgPivotY"] = json!(0.67);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let path = asset_path(&project, "background.png");
    assets
        .store_file_image(
            &ctx,
            &path,
            image::RgbaImage::from_pixel(17, 13, image::Rgba([31, 61, 121, 255])),
        )
        .unwrap();
    let texture = assets.texture(&ctx, &path).unwrap().id();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.grid = false;
    options.details = false;

    for (name, view) in [
        (
            "center",
            Viewport {
                center: [1920., 2048.],
                zoom: 4.,
            },
        ),
        (
            "edge",
            Viewport {
                center: [4000., 3980.],
                zoom: 1.375,
            },
        ),
    ] {
        let checksum = |output: &egui::FullOutput| {
            let (clip, mesh) = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                        Some((shape.clip_rect, mesh))
                    }
                    _ => None,
                })
                .unwrap();
            let mut hasher = DefaultHasher::new();
            let mut count = 0;
            for quad in mesh.vertices.as_chunks::<4>().0 {
                let bounds = Rect::from_min_max(quad[0].pos, quad[3].pos).intersect(clip);
                if bounds.is_positive() {
                    format!("{quad:?}").hash(&mut hasher);
                    count += 1;
                }
            }
            assert!(count > 100);
            (count, hasher.finish(), mesh.vertices.len() / 4)
        };
        let (_, first) = frame(&ctx, &project, &view, &options, &mut assets);
        let (visible, hash, total) = checksum(&first);
        drop(first);
        let mut times = [Duration::ZERO; 5];
        for time in &mut times {
            let start = Instant::now();
            let (_, output) = frame(&ctx, &project, &view, &options, &mut assets);
            *time = start.elapsed();
            assert_eq!(checksum(&output), (visible, hash, total));
        }
        times.sort_unstable();
        eprintln!(
            "Repeating background {name}: median {:?}; quads {visible}/{total} {hash:016x}; samples {times:?}",
            times[2]
        );
    }
}

#[test]
#[ignore = "manual large-world canvas performance comparison"]
fn benchmark_many_level_canvas_frames() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let selected = project.level_keys()[0];
    let template = project.level(selected).unwrap().clone();
    project.json["worldLayout"] = json!("Free");
    project.json["levels"] = (0..8192)
        .map(|i| {
            let mut level = template.clone();
            level["uid"] = json!(i + 1);
            level["iid"] = json!(format!("level-{i}"));
            level["identifier"] = json!(format!("Level_{i}"));
            level["worldX"] = json!(i % 128 * 256);
            level["worldY"] = json!(i / 128 * 256);
            level["worldDepth"] = json!(i * 7 % 9 - 4);
            level["__bgColor"] = json!(format!("#{:06x}", i * 733));
            level
        })
        .collect::<Vec<_>>()
        .into();
    let view = Viewport {
        center: [290., 210.],
        zoom: 1.,
    };
    let hidden = HashSet::new();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let mut options = options(&hidden, selected);
    options.world_depth = Some(-4);
    options.world_hover = Some(LevelKey {
        world: None,
        level: 9,
    });

    for world in [false, true] {
        options.world = world;
        let (_, expected) = frame(&ctx, &project, &view, &options, &mut assets);
        assert!(expected.shapes.len() > 8);
        let mut hasher = DefaultHasher::new();
        format!("{:?}", expected.shapes).hash(&mut hasher);
        let mut times = [Duration::ZERO; 10];
        for time in &mut times {
            let start = Instant::now();
            let (_, output) = frame(&ctx, &project, &view, &options, &mut assets);
            *time = start.elapsed();
            assert_eq!(output.shapes, expected.shapes);
        }
        times.sort_unstable();
        let scene = if world { "world" } else { "level" };
        eprintln!(
            "Many-level {scene} canvas: median {:?}; shapes {} {:016x}; samples {times:?}",
            times[times.len() / 2],
            expected.shapes.len(),
            hasher.finish()
        );
    }
}

#[test]
#[ignore = "manual entity depth sorting performance comparison"]
fn benchmark_entity_depth_sort() {
    use serde_json::json;

    let depth = |i: usize| ((i * 5129) % 991) as i64 - 540;
    let entities: Vec<_> = (0..8192).map(|i| json!({"px":[0,depth(i)]})).collect();
    for direction in ["YAscending", "YDescending"] {
        let def = json!({"entityDepthSorting":direction});
        let descending = direction == "YDescending";
        let mut expected: Vec<_> = (0..entities.len()).collect();
        expected.sort_unstable_by_key(|&i| (if descending { -depth(i) } else { depth(i) }, i));
        let mut times = [Duration::ZERO; 10];
        for time in &mut times {
            let start = Instant::now();
            let order = entity_order(&entities, &def);
            *time = start.elapsed();
            assert_eq!(order, expected);
        }
        times.sort_unstable();
        eprintln!(
            "Entity sort {direction}: median {:?}; samples {times:?}",
            times[times.len() / 2]
        );
    }
}

#[test]
#[ignore = "manual cropped World IntGrid performance comparison"]
fn benchmark_cropped_world_intgrid() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    let side = 512;
    project
        .resize_level(key, side * 16, side * 16, 0, 0)
        .unwrap();
    project.add_layer("Collision", "IntGrid", 16).unwrap();
    project.add_layer("Decoration", "IntGrid", 16).unwrap();
    for index in 0..2 {
        project.json["defs"]["layers"][index]["intGridValues"] =
            json!([{"value":1,"color":"#123456"},{"value":2,"color":"#AB4567"}]);
        let layer = project.layer_mut(key, index).unwrap();
        layer["__pxTotalOffsetX"] = json!(-9);
        layer["__pxTotalOffsetY"] = json!(5);
        layer["intGridCsv"] = (0..side * side - 5)
            .map(|i| json!((i + index as i32) % 3))
            .collect::<Vec<_>>()
            .into();
    }

    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.world = true;
    options.details = false;
    options.grid = false;
    let views = [
        Viewport {
            center: [4000., 4000.],
            zoom: 2.,
        },
        Viewport {
            center: [8070., 8050.],
            zoom: 1.25,
        },
        Viewport {
            center: [4096., 4096.],
            zoom: 0.16,
        },
    ];
    for (index, view) in views.iter().enumerate() {
        let (count, expected) = frame(&ctx, &project, view, &options, &mut assets);
        let expected_range = if index < 2 {
            500..2000
        } else {
            80_000..120_000
        };
        assert!(
            expected_range.contains(&count),
            "unexpected visible cell count {count}"
        );
        let mut hasher = DefaultHasher::new();
        format!("{:?}", expected.shapes).hash(&mut hasher);
        let mut times = [Duration::ZERO; 5];
        for time in &mut times {
            let start = Instant::now();
            let (actual_count, output) = frame(&ctx, &project, view, &options, &mut assets);
            *time = start.elapsed();
            assert_eq!(actual_count, count);
            assert_eq!(output.shapes, expected.shapes);
        }
        times.sort_unstable();
        eprintln!(
            "World IntGrid view {index}: median {:?}; cells {count} {:016x}; samples {times:?}",
            times[2],
            hasher.finish()
        );
    }
}

#[test]
#[ignore = "manual editing IntGrid frame performance comparison"]
fn benchmark_editing_intgrid_frames() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    let side = 160;
    project
        .resize_level(key, side * 16, side * 16, 0, 0)
        .unwrap();
    let uid = project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let def = &mut project.json["defs"]["layers"][0];
    def["intGridValues"] = json!([{"value":1,"color":"#123456"},{"value":2,"color":"#AB4567"}]);
    def["displayOpacity"] = json!(0.75);
    def["parallaxFactorX"] = json!(0.15);
    def["parallaxFactorY"] = json!(0.07);
    def["parallaxScaling"] = json!(true);
    let layer = project.layer_mut(key, 0).unwrap();
    layer["__pxTotalOffsetX"] = json!(-9);
    layer["__pxTotalOffsetY"] = json!(5);
    layer["intGridCsv"] = (0..side * side - 5)
        .map(|i| json!(i % 3))
        .collect::<Vec<_>>()
        .into();

    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.details = true;
    options.grid = false;
    options.rulers = false;
    let preview = AsyncPaintPreview {
        level: key,
        deadline: f64::INFINITY,
        layers: [(
            uid,
            AsyncLayerPreview {
                old_intgrid: [([47, 51], 2), ([50, 51], 0), ([85, 74], 2)].into(),
                ..Default::default()
            },
        )]
        .into(),
        hint_grid: None,
        hints: Default::default(),
    };
    for (case, zoom, pending) in [
        ("full", 0.25, false),
        ("zoomed", 2.5, false),
        ("preview", 0.25, true),
    ] {
        options.async_paint = pending.then_some(&preview);
        let view = Viewport {
            center: [1280.25, 1280.5],
            zoom,
        };
        let (count, expected) = frame(&ctx, &project, &view, &options, &mut assets);
        assert!(count > if zoom > 1. { 300 } else { 16_000 });
        let mut hasher = DefaultHasher::new();
        format!("{:?}", expected.shapes).hash(&mut hasher);
        let mut times = [Duration::ZERO; 7];
        for time in &mut times {
            let start = Instant::now();
            let (actual_count, output) = frame(&ctx, &project, &view, &options, &mut assets);
            *time = start.elapsed();
            assert_eq!(actual_count, count);
            assert_eq!(output.shapes, expected.shapes);
        }
        times.sort_unstable();
        eprintln!(
            "Editing IntGrid {case}: median {:?}; cells {count}; shapes {:016x}",
            times[3],
            hasher.finish()
        );
    }
}

#[test]
fn editing_intgrid_preview_merges_overrides_in_paint_order() {
    use serde_json::json;

    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 48, 32, 0, 0).unwrap();
    let uid = project.add_layer("Terrain", "IntGrid", 16).unwrap();
    project.json["defs"]["layers"][0]["intGridValues"] = json!([
        {"value":1,"color":"#ff0000"},{"value":2,"color":"#00ff00"},
        {"value":256,"color":"#0000ff"},{"value":-3,"color":"#ffff00"}
    ]);
    project.layer_mut(key, 0).unwrap()["intGridCsv"] = json!([1, 1, 1, 1, 1, 1]);
    let preview = AsyncPaintPreview {
        level: key,
        deadline: f64::INFINITY,
        hint_grid: None,
        hints: Default::default(),
        layers: [(
            uid,
            AsyncLayerPreview {
                old_intgrid: [([1, 0], 2), ([2, 0], 256), ([0, 1], 0), ([1, 1], -3)].into(),
                old_intgrid_cells: Some(vec![
                    json!(256),
                    json!(-3),
                    json!(0),
                    json!(1),
                    json!(1),
                    json!(2),
                ]),
                ..Default::default()
            },
        )]
        .into(),
    };
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.async_paint = Some(&preview);
    options.grid = false;
    options.rulers = false;
    let view = Viewport {
        center: [24., 16.],
        zoom: 1.,
    };
    let (count, output) = frame(
        &egui::Context::default(),
        &project,
        &view,
        &options,
        &mut Assets::default(),
    );
    assert_eq!(count, 5);
    let colors = [Color32::BLUE, Color32::GREEN, Color32::YELLOW, Color32::RED];
    let fills: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if colors.contains(&rect.fill) => Some(rect.fill),
            _ => None,
        })
        .collect();
    assert_eq!(
        fills,
        [
            Color32::BLUE,
            Color32::GREEN,
            Color32::BLUE,
            Color32::YELLOW,
            Color32::GREEN,
        ]
    );
}

#[test]
fn world_intgrid_overlap_starts_at_the_inclusive_viewport_edge() {
    use serde_json::json;

    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 16, 16, 0, 0).unwrap();
    project.add_layer("Bottom", "IntGrid", 16).unwrap();
    project.add_layer("Top", "IntGrid", 16).unwrap();
    for (index, color) in ["#ff0000", "#00ff00"].into_iter().enumerate() {
        project.json["defs"]["layers"][index]["intGridValues"] = json!([{"value":1,"color":color}]);
        project.layer_mut(key, index).unwrap()["intGridCsv"] = json!([1]);
    }
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.world = true;
    options.details = false;
    options.grid = false;
    let view = Viewport {
        center: [8., 8.],
        zoom: 1.,
    };
    for (offset, visible, hidden) in [
        (-509, Color32::GREEN, Color32::RED),
        (-508, Color32::RED, Color32::GREEN),
    ] {
        project.layer_mut(key, 0).unwrap()["__pxTotalOffsetX"] = json!(offset);
        let (count, output) = frame(&ctx, &project, &view, &options, &mut assets);
        assert_eq!(count, 1);
        let has_fill = |fill| {
            output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == fill))
        };
        assert!(has_fill(visible), "missing {visible:?} at offset {offset}");
        assert!(
            !has_fill(hidden),
            "unexpected {hidden:?} at offset {offset}"
        );
    }
}

#[test]
#[ignore = "manual world tileset preview performance comparison"]
fn benchmark_world_tileset_layers() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    let side = 256;
    project
        .resize_level(key, side * 16, side * 16, 0, 0)
        .unwrap();
    project.add_layer("Generated", "AutoLayer", 16).unwrap();
    project.add_layer("HandPainted", "Tiles", 16).unwrap();
    project.json["defs"]["tilesets"] = json!([{
        "uid":100,"tileGridSize":16,"embedAtlas":"LdtkIcons","relPath":null,
        "cachedPixelData":{"averageColors":"fabc08abfb41"}
    }]);
    for (index, field) in ["gridTiles", "autoLayerTiles"].into_iter().enumerate() {
        let layer = project.layer_mut(key, index).unwrap();
        layer["__tilesetDefUid"] = json!(100);
        layer["__pxTotalOffsetX"] = json!(index as i32 * 12 - 8);
        let mut tiles = Vec::new();
        for cell in 0..side * side {
            let px = [cell % side * 16, cell / side * 16];
            tiles.push(json!({"px":px,"src":[0,0],"f":0,"t":index * 2,"d":[cell],"a":1.0}));
            if index == 0 && cell % 5 == 0 {
                tiles.push(json!({"px":px,"src":[16,0],"f":0,"t":1,"d":[cell],"a":1.0}));
            }
        }
        tiles.push(json!({"t":2,"px":[-16,-16]}));
        layer[field] = tiles.into();
    }

    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.world = true;
    options.details = false;
    options.grid = false;
    let views = [
        Viewport {
            center: [2040., 2300.],
            zoom: 2.,
        },
        Viewport {
            center: [2048., 2048.],
            zoom: 0.125,
        },
    ];
    for (index, view) in views.iter().enumerate() {
        let (count, expected) = frame(&ctx, &project, view, &options, &mut assets);
        assert!(count > 500);
        let mut hasher = DefaultHasher::new();
        format!("{:?}", expected.shapes).hash(&mut hasher);
        let mut times = [Duration::ZERO; 5];
        for time in &mut times {
            let start = Instant::now();
            let (actual_count, output) = frame(&ctx, &project, view, &options, &mut assets);
            *time = start.elapsed();
            assert_eq!(actual_count, count);
            assert_eq!(output.shapes, expected.shapes);
        }
        times.sort_unstable();
        eprintln!(
            "World tileset view {index}: median {:?}; cells {count} {:016x}; samples {times:?}",
            times[2],
            hasher.finish()
        );
    }
}

#[test]
#[ignore = "manual tile-tag frame performance comparison"]
fn benchmark_large_tile_tag_frame() {
    use serde_json::json;
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 64 * 16, 64 * 16, 0, 0).unwrap();
    project.add_layer("Ground", "Tiles", 16).unwrap();
    let layers = &mut project.json["defs"]["layers"];
    layers[0]["tilesetDefUid"] = json!(100);
    let values = (0..24)
        .map(|i| json!({"id":format!("Tag{i}"),"color":"#AB00CD"}))
        .collect::<Vec<_>>();
    let tags = (0..24)
        .map(|i| {
            let tile_ids = (0..16)
                .filter(|tile| (tile + i) % 6 == 0)
                .collect::<Vec<_>>();
            json!({"enumValueId":format!("Tag{i}"),"tileIds":tile_ids})
        })
        .collect::<Vec<_>>();
    project.json["defs"]["enums"] = json!([{"uid":101,"values":values}]);
    project.json["defs"]["tilesets"] = json!([{
        "uid":100,"relPath":null,"embedAtlas":"LdtkIcons","tileGridSize":16,
        "tagsSourceEnumUid":101,"enumTags":tags
    }]);
    project.layer_mut(key, 0).unwrap()["gridTiles"] = (0..64 * 64)
        .map(|i| json!({"px":[i % 64 * 16,i / 64 * 16],"src":[i % 16 * 16,0],"t":i % 16,"f":0}))
        .collect::<Vec<_>>()
        .into();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.grid = false;
    options.details = false;
    options.tile_tags = true;
    let view = Viewport {
        center: [512., 512.],
        zoom: 0.5,
    };
    let _ = frame(&ctx, &project, &view, &options, &mut assets);
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let start = Instant::now();
        let (count, output) = frame(&ctx, &project, &view, &options, &mut assets);
        *time = start.elapsed();
        assert_eq!(count, 4_096);
        let tags = output.shapes.iter().filter(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == Color32::from_rgb(171, 0, 205))
        }).count();
        assert_eq!(tags, 4_096 * 4);
    }
    times.sort_unstable();
    eprintln!(
        "Large tile tag frame: median {:?}; samples {times:?}",
        times[2]
    );

    let view = Viewport {
        center: [512., 512.],
        zoom: 4.,
    };
    let (count, output) = frame(&ctx, &project, &view, &options, &mut assets);
    let shapes = output.shapes.len();
    assert_eq!(count, 192);
    let painted = ctx.tessellate(output.shapes, output.pixels_per_point);
    let mut hasher = DefaultHasher::new();
    for primitive in &painted {
        let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive else {
            panic!("headless tile tag fixture should use only meshes");
        };
        format!("{:?} {mesh:?}", primitive.clip_rect).hash(&mut hasher);
    }
    for time in &mut times {
        let start = Instant::now();
        let (actual_count, output) = frame(&ctx, &project, &view, &options, &mut assets);
        *time = start.elapsed();
        assert_eq!(actual_count, count);
        assert_eq!(output.shapes.len(), shapes);
    }
    times.sort_unstable();
    eprintln!(
        "Cropped tile tag frame: median {:?}; tessellation {:016x}, shapes {shapes}; samples {times:?}",
        times[2],
        hasher.finish()
    );
}

#[test]
fn classic_tiles_with_an_unavailable_atlas_mark_each_occupied_cell_in_level_view() {
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 64, 48, 0, 0).unwrap();
    project.add_layer("Ground", "Tiles", 16).unwrap();
    let def = &mut project.json["defs"]["layers"][0];
    def["tilePivotX"] = serde_json::json!(0.5);
    def["tilePivotY"] = serde_json::json!(0.25);
    let layer = project.layer_mut(key, 0).unwrap();
    layer["__pxTotalOffsetX"] = serde_json::json!(7);
    layer["__pxTotalOffsetY"] = serde_json::json!(11);
    layer["gridTiles"] = serde_json::json!([
        {"px":[0,0],"src":[0,0],"t":0,"f":0,"d":[0]},
        {"px":[0,0],"src":[16,0],"t":1,"f":1,"d":[0]},
        {"px":[32,16],"src":[48,0],"t":3,"f":3,"d":[6]}
    ]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut draw_options = options(&hidden, key);
    draw_options.grid = false;
    draw_options.details = false;
    let view = Viewport {
        center: [32., 24.],
        zoom: 1.,
    };
    let (_, output) = frame(&ctx, &project, &view, &draw_options, &mut assets);
    let marker = output
        .textures_delta
        .set
        .iter()
        .find_map(|(id, delta)| {
            let egui::ImageData::Color(image) = &delta.image;
            (image.size == [16, 16]
                && image.pixels[0] == Color32::from_rgb(136, 0, 0)
                && image.pixels[7 * 16 + 7] == Color32::RED)
                .then_some(*id)
        })
        .expect("missing atlas marker texture");
    let quads = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == marker => Some(
                mesh.vertices
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|quad| quad[0].pos),
            ),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(quads, [egui::pos2(476., 334.), egui::pos2(508., 350.)]);
    draw_options.world = true;
    let (_, output) = frame(&ctx, &project, &view, &draw_options, &mut assets);
    assert!(!output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == marker)
    }));
}

#[test]
#[ignore = "requires original sample resources under .reference/ldtk"]
fn official_examples_render_real_meshes_without_mutating_document() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".reference/ldtk/app/extraFiles/samples");
    for name in [
        "Entities.ldtk",
        "AutoLayers_3_Mosaic.ldtk",
        "AutoLayers_7_Biomes.ldtk",
        "Typical_TopDown_example.ldtk",
    ] {
        let project = Project::load(root.join(name)).unwrap();
        let before = project.json.clone();
        let key = project.level_keys()[0];
        let ctx = egui::Context::default();
        let mut assets = Assets::default();
        let hidden = HashSet::new();
        let draw_options = options(&hidden, key);
        let mut view = Viewport::default();
        view.fit(bounds(&project, key, false), [1000., 700.], 1.0);
        let start = Instant::now();
        let mut count = 0;
        let mut primitives = 0;
        while start.elapsed() < Duration::from_secs(5) {
            let (tiles, out) = frame(&ctx, &project, &view, &draw_options, &mut assets);
            count = count.max(tiles);
            primitives = ctx.tessellate(out.shapes, out.pixels_per_point).len();
            if assets.stats().pending == 0 && count > 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(assets.errors.is_empty(), "{name}: {:?}", assets.errors);
        assert!(count > 0, "no tile output for {name}");
        assert!(primitives > 0);
        assert_eq!(project.json, before);
        let mut world_options = options(&hidden, key);
        world_options.world = true;
        let mut world_view = Viewport::default();
        world_view.fit(bounds(&project, key, true), [1000., 700.], 1.0);
        let (_, world_output) = frame(&ctx, &project, &world_view, &world_options, &mut assets);
        assert!(
            !ctx.tessellate(world_output.shapes, world_output.pixels_per_point)
                .is_empty()
        );
        assert_eq!(project.json, before);
        assert!(assets.stats().textures > 0);
        eprintln!(
            "{name}: {count} visible cells, {primitives} mesh batches, {} resident textures",
            assets.stats().textures
        );
    }
}
#[test]
fn entity_preview_uses_actual_tile_mesh_opacity() {
    let mut project = Project::new();
    project.json["defs"]["tilesets"] =
        serde_json::json!([{"uid":1,"relPath":null,"embedAtlas":"LdtkIcons","tileGridSize":16}]);
    let def = serde_json::json!({"uid":2,"identifier":"Icon","width":32,"height":32,"pivotX":0.5,"pivotY":1,"tileRect":{"tilesetUid":1,"x":0,"y":0,"w":16,"h":16},"renderMode":"Tile","tileRenderMode":"Repeat","tileOpacity":0.5,"fillOpacity":0,"lineOpacity":0});
    project.json["defs"]["entities"] = serde_json::json!([def]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let texture = assets.embedded(&ctx).unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(64., 64.));
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ctx| {
            entity_preview(
                ctx,
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &project,
                &project.json["defs"]["entities"][0],
                &mut assets,
            )
        },
    );
    let vertices = output
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Mesh(m) = &s.shape {
                (m.texture_id == texture.id()).then_some(m.vertices.as_slice())
            } else {
                None
            }
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(vertices.len(), 16);
    assert!(vertices.iter().all(|v| (126..=128).contains(&v.color.a())));
}
#[test]
#[ignore = "release renderer timing on original large fixture"]
fn original_large_project_frame_cost() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".reference/ldtk/tests/lotsOfEntities.ldtk");
    let project = Project::load(path).unwrap();
    let ctx = egui::Context::default();
    let key = project.level_keys()[0];
    let hidden = HashSet::new();
    let options = options(&hidden, key);
    let mut assets = Assets::default();
    let mut view = Viewport::default();
    view.fit(bounds(&project, key, false), [1000., 700.], 1.0);
    for _ in 0..20 {
        frame(&ctx, &project, &view, &options, &mut assets);
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    let mut count = 0;
    for _ in 0..30 {
        count = frame(&ctx, &project, &view, &options, &mut assets).0;
    }
    eprintln!(
        "Original lotsOfEntities 30 frames: {:?}; last frame {count} visible cells",
        started.elapsed()
    );
    assert!(assets.errors.is_empty());
}

#[test]
fn editing_coordinates_match_scaled_and_unscaled_camera_layer_offsets() {
    let mut project = Project::new();
    project.json["defs"]["layers"] = serde_json::json!([{"uid":1,"parallaxFactorX":0.5,"parallaxFactorY":0.25,"parallaxScaling":false}]);
    project.json["levels"] = serde_json::json!([{"pxWid":200,"pxHei":100,"layerInstances":[{"layerDefUid":1,"pxOffsetX":20,"pxOffsetY":-10,"__pxTotalOffsetX":24,"__pxTotalOffsetY":-8}]}]);
    let key = LevelKey {
        world: None,
        level: 0,
    };
    let view = Viewport {
        center: [140., 80.],
        zoom: 3.,
    };
    assert_eq!(
        layer_to_world(&project, key, 0, &view, [10., 20.]),
        [59., 18.25]
    );
    assert_eq!(
        world_to_layer(&project, key, 0, &view, [59., 18.25]),
        [10., 20.]
    );
    project.json["defs"]["layers"][0]["parallaxScaling"] = serde_json::json!(true);
    assert_eq!(
        layer_to_world(&project, key, 0, &view, [10., 20.]),
        [87., 26.]
    );
    assert_eq!(
        world_to_layer(&project, key, 0, &view, [87., 26.]),
        [10., 20.]
    );
}
#[test]
fn neighbor_distance_uses_axis_gap_not_euclidean_or_contact_only() {
    assert_eq!(
        level_bounds_distance([0., 0., 100., 100.], [100., 0., 200., 100.]),
        0.
    );
    assert_eq!(
        level_bounds_distance([0., 0., 100., 100.], [130., 140., 200., 200.]),
        40.
    );
    assert_eq!(
        level_bounds_distance([0., 0., 100., 100.], [-400., 20., -301., 100.]),
        301.
    );
}
#[test]
fn selection_ghost_mirrors_cells_about_group_bounds_and_preserves_project() {
    let mut project = Project::new();
    project.json["defs"]["layers"] =
        serde_json::json!([{"uid":1,"intGridValues":[{"value":1,"color":"#ff0000"}]}]);
    project.json["levels"] = serde_json::json!([{"pxWid":200,"pxHei":100,"layerInstances":[{"layerDefUid":1,"__type":"IntGrid","__gridSize":16,"__pxTotalOffsetX":8,"__pxTotalOffsetY":0}]}]);
    let group = serde_json::json!({"origin":[40,16],"size":[32,16],"layers":[{"layerDefUid":1,"origin":[40,16],"rect":[2,1,2,1],"clipboard":{"gridSize":16,"width":2,"height":1,"intGridCsv":[1,0]}}]});
    let before = project.json.clone();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    let canvas = Rect::from_min_size(Pos2::ZERO, egui::vec2(200., 100.));
    let view = Viewport {
        center: [100., 50.],
        zoom: 1.,
    };
    let out = ctx.run(
        egui::RawInput {
            screen_rect: Some(canvas),
            ..Default::default()
        },
        |ctx| {
            draw_group_ghost(
                ctx,
                &ctx.layer_painter(egui::LayerId::background()),
                canvas,
                &project,
                &view,
                key,
                &group,
                [16., 0.],
                [true, false],
                GhostMode::Move,
                &mut assets,
            )
        },
    );
    let rects = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Rect(r) = &s.shape {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(rects.len(), 1);
    assert_eq!(
        rects[0].rect,
        Rect::from_min_size(egui::pos2(72., 16.), egui::vec2(16., 16.))
    );
    assert_eq!(rects[0].fill.a(), 255);
    assert_eq!(project.json, before);
}

#[test]
fn neighbor_distances_match_64_installed_original_js_vectors() {
    let oracle: Value =
        serde_json::from_str(include_str!("../docs/status/renderer-oracle.json")).unwrap();
    for case in values(&oracle["distances"]) {
        let a = std::array::from_fn(|i| case["a"][i].as_f64().unwrap() as f32);
        let b = std::array::from_fn(|i| case["b"][i].as_f64().unwrap() as f32);
        assert_eq!(
            level_bounds_distance(a, b),
            case["expected"].as_f64().unwrap() as f32
        );
    }
}

#[test]
fn offset_tiles_follow_the_layer_mask_in_level_view_and_the_grid_in_world_view() {
    let mut project = Project::new();
    project.json["defs"]["layers"] = serde_json::json!([
        {"uid":1,"type":"Tiles","gridSize":16,"renderInWorldView":true}
    ]);
    project.json["defs"]["tilesets"] = serde_json::json!([
        {"uid":2,"relPath":"offset-tiles.png","tileGridSize":16}
    ]);
    project.json["levels"] = serde_json::json!([{
        "pxWid":32,"pxHei":35,"worldX":0,"worldY":0,
        "layerInstances":[{
            "layerDefUid":1,"__type":"Tiles","__gridSize":16,"__cWid":3,"__cHei":2,
            "__tilesetDefUid":2,"pxOffsetX":-4,"pxOffsetY":4,
            "__pxTotalOffsetX":-4,"__pxTotalOffsetY":4,
            "gridTiles":[{"px":[32,16],"src":[0,0],"t":0,"f":0,"d":[5],"a":1}]
        }]
    }]);
    let ctx = egui::Context::default();
    let path = asset_path(&project, "offset-tiles.png");
    let mut assets = Assets::default();
    assets
        .store_file_image(
            &ctx,
            &path,
            image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 255, 255])),
        )
        .unwrap();
    let texture = assets.texture(&ctx, &path).unwrap().id();
    let hidden = HashSet::new();
    let mut options = options(&hidden, project.level_keys()[0]);
    options.details = false;
    options.grid = false;
    let view = Viewport {
        center: [16., 17.],
        zoom: 1.,
    };
    let tile_bounds = |out: &egui::FullOutput, world: bool| {
        out.shapes
            .iter()
            .find_map(|shape| {
                let tile = match &shape.shape {
                    egui::Shape::Mesh(mesh) if !world && mesh.texture_id == texture => {
                        mesh.calc_bounds()
                    }
                    egui::Shape::Rect(rect)
                        if world && rect.fill == Color32::from_rgb(255, 0, 255) =>
                    {
                        rect.rect
                    }
                    _ => return None,
                };
                Some(tile.intersect(shape.clip_rect))
            })
            .expect("offset tile")
    };
    let mut bounds = Vec::new();
    for world in [false, true] {
        options.world = world;
        let (count, out) = frame(&ctx, &project, &view, &options, &mut assets);
        assert_eq!(count, 1);
        bounds.push(tile_bounds(&out, world));
    }
    assert_eq!(
        bounds,
        [
            Rect::from_min_max(egui::pos2(512., 353.), egui::pos2(520., 364.)),
            Rect::from_min_max(egui::pos2(512., 353.), egui::pos2(528., 369.)),
        ]
    );

    let mut neighbor = project.json["levels"][0].clone();
    neighbor["worldX"] = 32.into();
    project.json["worldLayout"] = "Free".into();
    project.json["levels"][0]["layerInstances"] = serde_json::json!([]);
    project.json["levels"]
        .as_array_mut()
        .unwrap()
        .push(neighbor);
    options.world = false;
    let (count, out) = frame(&ctx, &project, &view, &options, &mut assets);
    assert_eq!(count, 1);
    assert_eq!(
        tile_bounds(&out, false),
        Rect::from_min_max(egui::pos2(544., 353.), egui::pos2(560., 369.))
    );
}

#[test]
fn world_simplification_does_not_reveal_tiles_under_transparent_stack_top() {
    let mut project = Project::new();
    project.json["defs"]["layers"] = serde_json::json!([{"uid":1,"renderInWorldView":true},{"uid":2,"renderInWorldView":true,"intGridValues":[{"value":1,"color":"#00ff00"}]}]);
    project.json["defs"]["tilesets"] = serde_json::json!([{"uid":3,"embedAtlas":"LdtkIcons","tileGridSize":16,"cachedPixelData":{"averageColors":"ff000fff"}}]);
    project.json["levels"] = serde_json::json!([{"pxWid":16,"pxHei":16,"layerInstances":[{"layerDefUid":1,"__type":"Tiles","__gridSize":16,"__cWid":1,"__cHei":1,"__tilesetDefUid":3,"gridTiles":[{"px":[0,0],"t":0},{"px":[0,0],"t":1}]},{"layerDefUid":2,"__type":"IntGrid","__gridSize":16,"__cWid":1,"intGridCsv":[1]}]}]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    options.world = true;
    options.details = false;
    options.grid = false;
    let view = Viewport {
        center: [8., 8.],
        zoom: 4.,
    };
    for offscreen in [0, 128] {
        project.layer_mut(key, 0).unwrap()["gridTiles"]
            .as_array_mut()
            .unwrap()
            .extend((0..offscreen).map(|i| serde_json::json!({"t":0,"px":[-2048-i*16,0]})));
        let (count, out) = frame(&ctx, &project, &view, &options, &mut assets);
        assert_eq!(count, 1);
        assert!(
            out.shapes
                .iter()
                .any(|s| { matches!(&s.shape, egui::Shape::Rect(r) if r.fill == Color32::GREEN) }),
            "transparent topmost stack must expose next layer, not earlier tiles in same stack"
        );
    }
}

#[test]
fn world_view_excludes_entity_layers_even_when_enabled() {
    let mut project = Project::new();
    project.json["defs"]["layers"] = serde_json::json!([
        {"uid":1,"type":"Entities","renderInWorldView":true}
    ]);
    project.json["defs"]["entities"] = serde_json::json!([
        {"uid":2,"color":"#FF0000","fillOpacity":1.,"lineOpacity":0.}
    ]);
    project.json["levels"] = serde_json::json!([{
        "pxWid":32,"pxHei":32,
        "layerInstances":[{
            "layerDefUid":1,"__type":"Entities","__gridSize":16,
            "entityInstances":[{"defUid":2,"px":[0,0],"width":16,"height":16}]
        }]
    }]);
    let key = project.level_keys()[0];
    let hidden = HashSet::new();
    let mut options = options(&hidden, key);
    let ctx = egui::Context::default();
    let view = Viewport {
        center: [16., 16.],
        zoom: 4.,
    };
    let mut assets = Assets::default();
    let has_entity = |output: egui::FullOutput| {
        output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == Color32::RED),
        )
    };

    assert!(has_entity(
        frame(&ctx, &project, &view, &options, &mut assets).1
    ));
    options.world = true;
    assert!(!has_entity(
        frame(&ctx, &project, &view, &options, &mut assets).1
    ));
}

#[test]
fn tiles_only_hides_entities_and_plain_int_grid_but_keeps_automatic_layers() {
    let mut project = Project::new();
    project.json["defs"]["layers"] = serde_json::json!([
        {"uid":1,"type":"Entities"},
        {"uid":2,"type":"IntGrid","intGridValues":[{"value":1,"color":"#00ff00"}]},
        {"uid":3,"type":"IntGrid","tilesetDefUid":4,"intGridValues":[{"value":1,"color":"#ff0000"}]}
    ]);
    project.json["defs"]["tilesets"] = serde_json::json!([
        {"uid":4,"relPath":"auto.png","tileGridSize":16}
    ]);
    project.json["defs"]["entities"] = serde_json::json!([
        {"uid":5,"color":"#ff00ff","width":16,"height":16,"renderMode":"Rectangle"}
    ]);
    project.json["levels"] = serde_json::json!([{
        "pxWid":48,"pxHei":16,
        "layerInstances":[
            {"layerDefUid":1,"__type":"Entities","__gridSize":16,
             "entityInstances":[{"defUid":5,"px":[8,8],"width":16,"height":16}]},
            {"layerDefUid":2,"__type":"IntGrid","__gridSize":16,"__cWid":3,"__cHei":1,"intGridCsv":[0,1,0]},
            {"layerDefUid":3,"__type":"IntGrid","__gridSize":16,"__cWid":3,"__cHei":1,"__tilesetDefUid":4,
             "intGridCsv":[0,0,1],"autoLayerTiles":[{"px":[32,0],"src":[0,0],"t":0,"f":0,"a":1}]}
        ]
    }]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let path = asset_path(&project, "auto.png");
    assets
        .store_file_image(
            &ctx,
            &path,
            image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 255, 255, 255])),
        )
        .unwrap();
    let texture = assets.texture(&ctx, &path).unwrap().id();
    let hidden = HashSet::new();
    let mut options = options(&hidden, project.level_keys()[0]);
    options.active_layer = 2;
    let view = Viewport {
        center: [24., 8.],
        zoom: 1.,
    };
    let has_fill = |output: &egui::FullOutput, color| {
        output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == color))
    };
    let has_tiles = |output: &egui::FullOutput| {
        output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == texture),
        )
    };
    let (_, all) = frame(&ctx, &project, &view, &options, &mut assets);
    assert!(has_fill(&all, Color32::GREEN));
    assert!(has_fill(&all, Color32::from_rgb(255, 0, 255)));
    options.details = false;
    options.grid = false;
    options.rulers = false;
    let (_, only_tiles) = frame(&ctx, &project, &view, &options, &mut assets);
    assert!(!has_fill(&only_tiles, Color32::GREEN));
    assert!(!has_fill(&only_tiles, Color32::from_rgb(255, 0, 255)));
    assert!(has_tiles(&only_tiles));

    options.render_auto = false;
    let (_, underlying) = frame(&ctx, &project, &view, &options, &mut assets);
    assert!(has_fill(&underlying, Color32::RED));
    assert!(!has_tiles(&underlying));

    options.render_auto = true;
    project.json["defs"]["tilesets"][0]["relPath"] = "missing-auto.png".into();
    let (_, missing) = frame(&ctx, &project, &view, &options, &mut assets);
    assert!(has_fill(&missing, Color32::RED));
    assert!(!has_tiles(&missing));
}

#[test]
fn world_view_regenerates_missing_average_cache_from_original_pixels() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("atlas.png");
    image::save_buffer(
        &path,
        &[
            255, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255,
        ],
        2,
        2,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new();
    project.path = Some(temp.path().join("fixture.ldtk"));
    project.json["defs"]["layers"] = serde_json::json!([{"uid":1,"renderInWorldView":true}]);
    project.json["defs"]["tilesets"] =
        serde_json::json!([{"uid":2,"relPath":"atlas.png","tileGridSize":2}]);
    project.json["levels"] = serde_json::json!([{"pxWid":16,"pxHei":16,"layerInstances":[{"layerDefUid":1,"__type":"Tiles","__gridSize":16,"__tilesetDefUid":2,"gridTiles":[{"px":[0,0],"src":[0,0],"t":0}]}]}]);
    let before = project.json.clone();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(
        &hidden,
        LevelKey {
            world: None,
            level: 0,
        },
    );
    options.world = true;
    options.details = false;
    options.grid = false;
    let view = Viewport {
        center: [8., 8.],
        zoom: 4.,
    };
    let started = Instant::now();
    loop {
        let (count, out) = frame(&ctx, &project, &view, &options, &mut assets);
        if count > 0 {
            assert!(out.shapes.iter().any(
                |s| matches!(&s.shape,egui::Shape::Rect(r) if r.fill==Color32::from_rgb(180,0,180))
            ));
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            assets.errors
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(project.json, before);
}
#[test]
fn nearby_view_shows_separated_levels_until_300px_and_hides_upper_depth() {
    let mut project = Project::new();
    project.json["worldLayout"] = serde_json::json!("Free");
    project.json["defs"]["layers"] = serde_json::json!([{"uid":1,"renderInWorldView":true,"intGridValues":[{"value":1,"color":"#00ff00"}]}]);
    project.json["levels"] = serde_json::json!([{"worldX":0,"worldY":0,"worldDepth":0,"pxWid":16,"pxHei":16,"layerInstances":[]},{"worldX":48,"worldY":0,"worldDepth":0,"pxWid":16,"pxHei":16,"layerInstances":[{"layerDefUid":1,"__type":"IntGrid","__gridSize":16,"__cWid":1,"intGridCsv":[1]}]}]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(
        &hidden,
        LevelKey {
            world: None,
            level: 0,
        },
    );
    options.details = false;
    options.grid = false;
    let view = Viewport {
        center: [100., 8.],
        zoom: 1.,
    };
    assert_eq!(frame(&ctx, &project, &view, &options, &mut assets).0, 1);
    project.json["levels"][1]["worldX"] = serde_json::json!(316);
    assert_eq!(frame(&ctx, &project, &view, &options, &mut assets).0, 1);
    project.json["levels"][1]["worldX"] = serde_json::json!(317);
    assert_eq!(frame(&ctx, &project, &view, &options, &mut assets).0, 0);
    project.json["levels"][1]["worldX"] = serde_json::json!(48);
    project.json["levels"][1]["worldDepth"] = serde_json::json!(-1);
    assert_eq!(frame(&ctx, &project, &view, &options, &mut assets).0, 0);
}

#[test]
fn linear_world_identifier_plate_rotates_with_its_original_bitmap_text() {
    let mut project = Project::new();
    project.path = Some("Example.ldtk".into());
    project.json["worldLayout"] = serde_json::json!("LinearHorizontal");
    project.json["levels"] = serde_json::json!([{"identifier":"Room","worldX":0,"worldY":0,"pxWid":128,"pxHei":128,"layerInstances":[]}]);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut options = options(
        &hidden,
        LevelKey {
            world: None,
            level: 0,
        },
    );
    options.world = true;
    let view = Viewport {
        center: [64., 64.],
        zoom: 1.,
    };
    let (_, out) = frame(&ctx, &project, &view, &options, &mut assets);
    let plate = out
        .shapes
        .iter()
        .find_map(|s| {
            if let egui::Shape::Path(p) = &s.shape {
                (p.closed && p.points.len() == 4).then_some(p)
            } else {
                None
            }
        })
        .expect("rotated identifier plate");
    let edge = plate.points[1] - plate.points[0];
    assert!((edge.y.atan2(edge.x) + 0.4).abs() < 0.00001);
    let title = assets
        .builtin(
            &ctx,
            "font-title-90",
            include_bytes!(
                "../assets/bitmap_fonts/noto_sans_display_semicondensed_extralight_90.png"
            ),
        )
        .unwrap();
    assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Mesh(m) if m.texture_id==title.id()&&!m.vertices.is_empty())));
    let stripes = image::load_from_memory(include_bytes!("../assets/img/world-large-stripes.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!(stripes.dimensions(), (234, 234));
    assert!(stripes.pixels().any(|p| p.0[3] == 0));
    assert!(stripes.pixels().any(|p| p.0[3] > 0));
}

#[test]
fn selection_entity_ghost_uses_half_alpha_and_preserves_normal_scene() {
    let mut project = Project::new();
    project.add_layer("Entities", "Entities", 16).unwrap();
    let uid = project.add_entity_def("Actor", 16, 16).unwrap();
    project.json["defs"]["entities"][0]["color"] = serde_json::json!("#ff0000");
    project.json["defs"]["entities"][0]["showName"] = serde_json::json!(true);
    let key = project.level_keys()[0];
    project.add_entity(key, 0, uid, 32, 32).unwrap();
    project.add_entity(key, 0, uid, 80, 32).unwrap();
    let moving = project.layer(key, 0).unwrap()["entityInstances"][0].clone();
    let view = Viewport {
        center: [128., 128.],
        zoom: 1.,
    };
    let hidden = HashSet::new();
    let mut opts = options(&hidden, key);
    opts.grid = false;
    opts.rulers = false;
    opts.guides = false;
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let body_count = |out: &egui::FullOutput| {
        out.shapes.iter().filter(|shape|matches!(&shape.shape,egui::Shape::Rect(r) if r.rect.size()==egui::vec2(16.,16.)&&r.fill.r()>0&&r.fill.g()==0&&r.fill.b()==0)).count()
    };
    assert_eq!(
        body_count(&frame(&ctx, &project, &view, &opts, &mut assets).1),
        2
    );
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 700.));
    let group = serde_json::json!({"origin":[0,0],"size":[256,256],"layers":[{"layerDefUid":project.layer(key,0).unwrap()["layerDefUid"],"origin":[0,0],"rect":[0,0,16,16],"clipboard":{"gridSize":16,"entityInstances":[moving]}}]});
    for (mode, alpha) in [
        (GhostMode::Move, 128u8),
        (GhostMode::Copy, 128u8),
        (GhostMode::Brush, 84u8),
    ] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                draw_group_ghost(
                    ctx,
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &project,
                    &view,
                    key,
                    &group,
                    [16., 0.],
                    [false; 2],
                    mode,
                    &mut assets,
                )
            },
        );
        assert_eq!(body_count(&out), 1);
        let body = out
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.size() == egui::vec2(16., 16.) && rect.fill.r() > 0 =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        assert!((alpha - 1..=alpha + 1).contains(&body.fill.a()));
        assert!(
            !out.shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Mesh(_))),
            "moving entity core does not include name/field labels"
        );
    }
}

#[test]
fn brush_and_selection_tile_ghosts_use_source_anchors_and_keep_flipped_uvs() {
    let mut project = Project::new();
    project.json["defs"]["tilesets"] =
        serde_json::json!([{"uid":1,"relPath":null,"embedAtlas":"LdtkIcons","tileGridSize":32}]);
    project.json["defs"]["layers"] =
        serde_json::json!([{"uid":2,"type":"Tiles","tilePivotX":0.25,"tilePivotY":0.75}]);
    project.json["levels"][0]["layerInstances"] = serde_json::json!([{"layerDefUid":2,"__type":"Tiles","__gridSize":16,"__tilesetDefUid":1,"__pxTotalOffsetX":8,"__pxTotalOffsetY":4}]);
    let group = serde_json::json!({"origin":[80,32],"size":[16,16],"layers":[{"layerDefUid":2,"origin":[80,32],"rect":[0,0,1,1],"clipboard":{"gridSize":16,"width":1,"height":1,"gridTiles":[{"px":[0,0],"src":[32,0],"t":1,"f":1,"a":1}]}}]});
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let atlas = assets.embedded(&ctx).unwrap();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200., 100.));
    let view = Viewport {
        center: [100., 50.],
        zoom: 1.,
    };
    let key = project.level_keys()[0];
    for (mode, want) in [
        (GhostMode::Move, [64., 32., 96., 64.]),
        (GhostMode::Brush, [88., 8., 120., 40.]),
    ] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                draw_group_ghost(
                    ctx,
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &project,
                    &view,
                    key,
                    &group,
                    [0.; 2],
                    [false; 2],
                    mode,
                    &mut assets,
                )
            },
        );
        let mesh = out
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == atlas.id() => Some(mesh),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            mesh.calc_bounds(),
            Rect::from_min_max(egui::pos2(want[0], want[1]), egui::pos2(want[2], want[3]))
        );
        assert_eq!(
            mesh.vertices[0].uv,
            egui::pos2(64. / atlas.size()[0] as f32, 0.)
        );
        assert_eq!(mesh.vertices[1].uv.x, 32. / atlas.size()[0] as f32);
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.color == Color32::WHITE)
        );
    }
}

#[test]
fn geometry_aware_ghost_flip_mirrors_inside_each_entry_and_brush_grid_is_outline_only() {
    let mut project = Project::new();
    project.json["defs"]["layers"] =
        serde_json::json!([{"uid":1,"intGridValues":[{"value":1,"color":"#ff0000"}]}]);
    project.json["levels"][0]["layerInstances"] = serde_json::json!([{"layerDefUid":1,"__type":"IntGrid","__gridSize":16,"__pxTotalOffsetX":8,"__pxTotalOffsetY":0}]);
    let group = serde_json::json!({"geometryAware":true,"scaledGridSize":16,"origin":[0,0],"size":[200,100],"layers":[{"layerDefUid":1,"origin":[40,16],"geometry":{"scale":1},"rect":[2,1,2,1],"clipboard":{"gridSize":16,"width":2,"height":1,"intGridCsv":[1,0]}}]});
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(200., 100.));
    let view = Viewport {
        center: [100., 50.],
        zoom: 1.,
    };
    let key = project.level_keys()[0];
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    for mode in [GhostMode::Move, GhostMode::Brush] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                draw_group_ghost(
                    ctx,
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &project,
                    &view,
                    key,
                    &group,
                    [16., 0.],
                    [true, false],
                    mode,
                    &mut assets,
                )
            },
        );
        let rects = out
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        let outer = rects.last().unwrap();
        assert_eq!(outer.rect.min, egui::pos2(72., 16.));
        if mode == GhostMode::Brush {
            assert_eq!(rects.len(), 2);
            assert!(rects.iter().all(|r| r.fill == Color32::TRANSPARENT));
            assert_eq!(outer.stroke.width, 2.);
        } else {
            assert_eq!(outer.fill, Color32::RED);
        }
    }
}

#[test]
fn preview_154_point_path_loop_counts_null_slots_when_returning_to_the_entity() {
    let mut project = Project::new();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let actor = project.add_entity_def("Actor", 16, 16).unwrap();
    let field = project
        .add_field_definition(Some(actor), "Path", "Point", true)
        .unwrap();
    project
        .update_field_definition(
            field,
            &serde_json::json!({
                "canBeNull":true,"editorAlwaysShow":true,"editorDisplayMode":"PointPathLoop",
                "editorLinkStyle":"DashedLine","editorDisplayColor":"#CC1166"
            }),
        )
        .unwrap();
    let key = project.level_keys()[0];
    project.add_entity(key, 0, actor, 16, 16).unwrap();
    let iid = project.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let hidden = HashSet::new();
    let mut draw_options = options(&hidden, key);
    draw_options.guides = false;
    draw_options.grid = false;
    let view = Viewport {
        center: [128., 128.],
        zoom: 1.,
    };
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let clip = [1000., 700.];
    let start = view.to_screen([24., 24.], [0., 0.], clip);
    let end = view.to_screen([72., 24.], [0., 0.], clip);
    for value in [
        serde_json::json!([{"cx":4,"cy":1}]),
        serde_json::json!([null,{"cx":4,"cy":1}]),
    ] {
        let has_loop = value.as_array().unwrap().len() > 1;
        project
            .set_field_editor_value(key, Some(&iid), field, value)
            .unwrap();
        let (_, output) = frame(&ctx, &project, &view, &draw_options, &mut assets);
        let direction = |forward: bool| {
            output.shapes.iter().any(|item| {
                let egui::Shape::LineSegment { points, stroke } = &item.shape else {
                    return false;
                };
                stroke.color.r() > 130
                    && stroke.color.b() > 55
                    && stroke.color.g() < 35
                    && points
                        .iter()
                        .all(|p| p.y == start[1] && (start[0]..=end[0]).contains(&p.x))
                    && (if forward {
                        points[1].x > points[0].x
                    } else {
                        points[1].x < points[0].x
                    })
            })
        };
        assert!(direction(true), "point path didn't reach its visible point");
        assert_eq!(
            direction(false),
            has_loop,
            "return segment for {has_loop} loop slots"
        );
    }
}

#[test]
fn point_ghosts_draw_selected_circles_and_move_links_but_copy_has_no_links() {
    let mut project = Project::new();
    project.add_layer("Entities", "Entities", 16).unwrap();
    let uid = project.add_entity_def("Actor", 16, 16).unwrap();
    project.json["defs"]["entities"][0]["pivotX"] = serde_json::json!(0);
    project.json["defs"]["entities"][0]["pivotY"] = serde_json::json!(0);
    project.json["defs"]["entities"][0]["fieldDefs"] = serde_json::json!([{"uid":99,"type":"F_Point","isArray":true,"editorDisplayMode":"PointPath"}]);
    let key = project.level_keys()[0];
    project.add_entity(key, 0, uid, 16, 16).unwrap();
    project.layer_mut(key, 0).unwrap()["entityInstances"][0]["fieldInstances"] = serde_json::json!([{"defUid":99,"__type":"Array<Point>","__value":[{"cx":2,"cy":1},{"cx":4,"cy":1}]}]);
    let entity = project.layer(key, 0).unwrap()["entityInstances"][0].clone();
    let group = serde_json::json!({"origin":[0,0],"size":[256,256],"layers":[{"layerDefUid":project.layer(key,0).unwrap()["layerDefUid"],"origin":[0,0],"rect":[0,0,16,16],"clipboard":{"gridSize":16,"entityInstances":[entity]},"points":[{"entityIid":entity["iid"],"fieldDefUid":99,"arrayIndex":0,"value":{"cx":2,"cy":1}}]}]});
    let before = project.json.clone();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(256., 256.));
    let view = Viewport {
        center: [128., 128.],
        zoom: 1.,
    };
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    for mode in [GhostMode::Move, GhostMode::Copy] {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                draw_group_ghost(
                    ctx,
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &project,
                    &view,
                    key,
                    &group,
                    [16., 0.],
                    [false; 2],
                    mode,
                    &mut assets,
                )
            },
        );
        let circles = out
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) => Some(circle),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(circles.len(), 2);
        assert_eq!(circles[0].center, egui::pos2(56., 24.));
        assert_eq!(circles[0].radius, 8.);
        assert_eq!(circles[1].radius, 4.8);
        let lines = out
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, .. } => Some(*points),
                _ => None,
            })
            .collect::<Vec<_>>();
        if mode == GhostMode::Move {
            assert_eq!(lines.len(), 3);
            assert!(lines.contains(&[egui::pos2(40., 24.), egui::pos2(56., 24.)]));
            assert!(lines.contains(&[egui::pos2(56., 24.), egui::pos2(72., 24.)]));
        } else {
            assert!(lines.is_empty());
        }
    }
    assert_eq!(project.json, before);
}

#[test]
fn source_ruler_font_scales_with_level_root_not_screen_field_root() {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(2.);
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    assert_eq!(ruler_scale(&ctx, 1., 816.), 2.);
    assert_eq!(ruler_scale(&ctx, 0.5, 100.), 0.5);
}

#[test]
fn source_ruler_hides_only_the_bottom_label_for_a_displayed_beneath_field() {
    let mut project = Project::new();
    let key = project.level_keys()[0];
    let uid = project
        .add_field_definition(None, "Caption", "String", false)
        .unwrap();
    project
        .update_field_definition(
            uid,
            &serde_json::json!({
                "editorDisplayMode":"ValueOnly",
                "editorDisplayPos":"Beneath"
            }),
        )
        .unwrap();
    let view = Viewport {
        center: [128.; 2],
        zoom: 1.,
    };
    let hidden = HashSet::new();
    let opts = options(&hidden, key);
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let bottom_label = |out: egui::FullOutput| {
        out.shapes.into_iter().any(|shape| {
            matches!(shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id != egui::TextureId::default() && mesh.calc_bounds().top() > 485.)
        })
    };
    assert!(bottom_label(
        frame(&ctx, &project, &view, &opts, &mut assets).1
    ));
    project
        .set_field_value(key, None, uid, serde_json::json!("Caption"))
        .unwrap();
    assert!(!bottom_label(
        frame(&ctx, &project, &view, &opts, &mut assets).1
    ));
    project
        .update_field_definition(uid, &serde_json::json!({"editorDisplayPos":"Above"}))
        .unwrap();
    assert!(bottom_label(
        frame(&ctx, &project, &view, &opts, &mut assets).1
    ));
}

#[test]
fn source_level_annotation_position_uses_the_world_origin_at_fractional_zoom() {
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 159, 127, 0, 0).unwrap();
    let level = project.level_mut(key).unwrap();
    level["worldX"] = 160.into();
    level["worldY"] = (-48).into();
    let uid = project
        .add_field_definition(None, "Caption", "String", false)
        .unwrap();
    project
        .update_field_definition(uid, &serde_json::json!({"editorDisplayMode":"ValueOnly"}))
        .unwrap();
    project
        .set_field_value(key, None, uid, serde_json::json!("A caption below"))
        .unwrap();
    let view = Viewport {
        center: [79.5, 63.5],
        zoom: 900. / 207.,
    };
    let hidden = HashSet::new();
    let mut opts = options(&hidden, key);
    opts.grid = false;
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1140., 900.));
    let out = ctx.run(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ctx| {
            draw(
                &ctx.layer_painter(egui::LayerId::background()),
                rect,
                &project,
                &view,
                &opts,
                None,
                &mut assets,
            );
        },
    );
    let mut annotation_y = out
        .shapes
        .iter()
        .filter_map(|shape| {
            let egui::Shape::Mesh(mesh) = &shape.shape else {
                return None;
            };
            let bounds = mesh.calc_bounds();
            (mesh.texture_id != egui::TextureId::default()
                && (500. ..650.).contains(&bounds.center().x)
                && (70. ..135.).contains(&bounds.top()))
            .then_some(bounds.top())
        })
        .collect::<Vec<_>>();
    annotation_y.sort_by(f32::total_cmp);
    // Runtime oracle at this camera position: the field flow starts at 86.39
    // and the name plate at 113.43. The bitmap glyphs add their own offsets.
    assert_eq!(annotation_y.len(), 2, "{annotation_y:?}");
    assert!((86. ..91.).contains(&annotation_y[0]), "{annotation_y:?}");
    assert!((114. ..117.).contains(&annotation_y[1]), "{annotation_y:?}");
}

#[test]
fn source_world_fields_use_truncated_world_positions_in_each_layout() {
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 159, 127, 0, 0).unwrap();
    let uid = project
        .add_field_definition(None, "Caption", "String", false)
        .unwrap();
    project
        .update_field_definition(uid, &serde_json::json!({"editorDisplayMode":"ValueOnly"}))
        .unwrap();
    project
        .set_field_value(key, None, uid, serde_json::json!("A caption below"))
        .unwrap();
    let level = project.level_mut(key).unwrap();
    level["worldX"] = 160.into();
    level["worldY"] = (-48).into();
    let hidden = HashSet::new();
    let mut opts = options(&hidden, key);
    opts.world = true;
    opts.grid = false;
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1140., 900.));
    // Original WorldRender's visible field container positions at these camera poses.
    for (layout, center, zoom, scale, field_origin) in [
        ("Free", [239.5, 15.5], 0.95, 1., egui::pos2(522.5, 496.75)),
        (
            "LinearHorizontal",
            [80.301_76, 63.5],
            1.247_264_7,
            1.,
            egui::pos2(522.385_1, 569.315_1),
        ),
        (
            "LinearHorizontal",
            [80.301_76, 63.5],
            1.247_264_7,
            0.5,
            egui::pos2(546.083_1, 569.315_1),
        ),
        (
            "LinearVertical",
            [80.052_08, 63.5],
            1.811_320_8,
            1.,
            egui::pos2(770.962_3, 443.679_3),
        ),
    ] {
        project.json["worldLayout"] = layout.into();
        project
            .update_field_definition(uid, &serde_json::json!({"editorDisplayScale":scale}))
            .unwrap();
        let view = Viewport { center, zoom };
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                draw(
                    &ctx.layer_painter(egui::LayerId::background()),
                    rect,
                    &project,
                    &view,
                    &opts,
                    None,
                    &mut assets,
                );
            },
        );
        let (field_index, glyph) = out
            .shapes
            .iter()
            .enumerate()
            .find_map(|(index, shape)| {
                let egui::Shape::Mesh(mesh) = &shape.shape else {
                    return None;
                };
                let glyph_vertices = 15 * 4; // Visible glyphs, including quotes.
                (mesh.texture_id != egui::TextureId::default()
                    && matches!(mesh.vertices.len(), 60 | 540)
                    && mesh.calc_bounds().min.distance(field_origin) < 10.)
                    .then(|| {
                        (
                            index,
                            mesh.vertices[mesh.vertices.len() - glyph_vertices].pos,
                        )
                    })
            })
            .expect(layout);
        assert!(
            glyph.distance(field_origin + egui::vec2(0., 2. * scale)) < 0.01,
            "{layout} at {scale}×: {glyph:?}"
        );
        let highlight = out.shapes.iter().position(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == theme::ACCENT)
        }).unwrap();
        assert!(
            highlight > field_index,
            "{layout}: selected outline must cover the fields"
        );
    }
}

#[test]
fn preview_154_guide_grid_uses_custom_color_and_opacity_only_when_color_is_set() {
    assert_eq!(
        guide_grid_color(&serde_json::json!({"guideOpacity":0.8}), Color32::WHITE),
        Color32::WHITE.gamma_multiply(0.33)
    );
    assert_eq!(
        guide_grid_color(&serde_json::json!({}), Color32::BLACK),
        Color32::BLACK.gamma_multiply(0.33)
    );
    assert_eq!(
        guide_grid_color(
            &serde_json::json!({"guideColor":"#FF0000","guideOpacity":0.5}),
            Color32::BLACK
        ),
        Color32::from_rgb(255, 0, 0).gamma_multiply(0.5)
    );
    assert_eq!(
        guide_grid_color(&serde_json::json!({"guideColor":"#FF0000"}), Color32::WHITE),
        Color32::from_rgb(255, 0, 0).gamma_multiply(0.33)
    );
}

#[test]
fn preview_154_custom_guide_color_reaches_actual_canvas_shapes() {
    let mut project = Project::new();
    let layer = project.add_layer("Grid", "IntGrid", 16).unwrap();
    project.update_layer_properties(layer, &serde_json::json!({"guideGridWid":32,"guideGridHei":32,"guideColor":"#FF0000","guideOpacity":0.5})).unwrap();
    let key = project.level_keys()[0];
    let hidden = HashSet::new();
    let options = options(&hidden, key);
    let ctx = egui::Context::default();
    let mut view = Viewport::default();
    view.fit(bounds(&project, key, false), [1000., 700.], 1.0);
    let (_, out) = frame(&ctx, &project, &view, &options, &mut Assets::default());
    let expected = Color32::from_rgb(255, 0, 0).gamma_multiply(0.5);
    assert!(out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { stroke, .. } if stroke.color == expected)), "custom guide grid lines must reach the canvas output");
}

#[test]
fn preview_154_filepath_field_renders_external_image_as_entity_tile() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("decal.png");
    image::save_buffer(&path, &[255; 8 * 4 * 4], 8, 4, image::ColorType::Rgba8).unwrap();
    let mut project = Project::new();
    project.path = Some(temp.path().join("fixture.ldtk"));
    project.add_layer("Actors", "Entities", 16).unwrap();
    let def = project.add_entity_def("Actor", 16, 16).unwrap();
    project
        .update_entity_definition(def, &serde_json::json!({"color": "#DF6247"}))
        .unwrap();
    let field = project
        .add_field_definition(Some(def), "Image", "FilePath", false)
        .unwrap();
    project
        .update_field_definition(
            field,
            &serde_json::json!({"editorDisplayMode":"EntityTile"}),
        )
        .unwrap();
    let key = project.level_keys()[0];
    project.add_entity(key, 0, def, 32, 32).unwrap();
    let iid = project.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    project
        .set_field_editor_value(key, Some(&iid), field, serde_json::json!("decal.png"))
        .unwrap();
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let opts = options(&hidden, key);
    let view = Viewport {
        center: [32., 32.],
        zoom: 4.,
    };
    let started = Instant::now();
    loop {
        let (_, out) = frame(&ctx, &project, &view, &opts, &mut assets);
        if let Some(texture) = assets.texture(&ctx, &path) {
            let decal = out
                .shapes
                .iter()
                .position(|shape| {
                    matches!(&shape.shape, egui::Shape::Mesh(mesh)
                        if mesh.texture_id == texture.id() && !mesh.vertices.is_empty())
                })
                .expect("FilePath image has a decal mesh");
            let pivot_rect = Rect::from_min_size(Pos2::new(500., 350.), egui::Vec2::splat(4.));
            let pivot = out
                .shapes
                .iter()
                .position(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.rect == pivot_rect && rect.fill == theme::color("#DF6247"))
                })
                .expect("entity has a one-pixel pivot marker");
            assert!(
                pivot < decal,
                "opaque decal pixels must cover the pivot marker"
            );
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            assets.errors
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn preview_154_world_multiselection_hides_current_level_highlight() {
    let project = Project::new();
    let key = project.level_keys()[0];
    let ctx = egui::Context::default();
    let mut assets = Assets::default();
    let hidden = HashSet::new();
    let mut opts = options(&hidden, key);
    opts.world = true;
    opts.details = false;
    let view = Viewport {
        center: [128., 128.],
        zoom: 1.,
    };
    let has_highlight = |output: &egui::FullOutput| {
        output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == theme::ACCENT))
    };
    let (_, single) = frame(&ctx, &project, &view, &opts, &mut assets);
    assert!(has_highlight(&single));
    opts.world_multi_selection = true;
    let (_, multiple) = frame(&ctx, &project, &view, &opts, &mut assets);
    assert!(!has_highlight(&multiple));
}
