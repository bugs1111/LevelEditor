use super::*;
use std::time::{Duration, Instant};

pub(super) fn options(hidden: &HashSet<i64>, selected: LevelKey) -> RenderOptions<'_> {
    RenderOptions {
        selected,
        active_layer: 0,
        world: false,
        grid: true,
        details: true,
        single_layer: false,
        hidden,
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
            count = draw(ctx, &painter, rect, project, view, options, assets);
        },
    );
    (count, output)
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
        view.fit(bounds(&project, key, false), [1000., 700.]);
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
        world_view.fit(bounds(&project, key, true), [1000., 700.]);
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
    view.fit(bounds(&project, key, false), [1000., 700.]);
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
            draw_selection_ghost(
                ctx,
                &ctx.layer_painter(egui::LayerId::background()),
                canvas,
                &project,
                &view,
                key,
                &group,
                [16., 0.],
                [true, false],
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
    let (count, out) = frame(&ctx, &project, &view, &options, &mut assets);
    assert_eq!(count, 1);
    let fills = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Rect(r) = &s.shape {
                Some(r.fill)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        fills.contains(&Color32::GREEN),
        "transparent topmost stack must expose next layer, not earlier tiles in same stack"
    );
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
    opts.details = false;
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
