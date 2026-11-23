use super::*;
use super::field_definition_form::source_file_types;

#[test]
fn backup_panels_keep_the_form_visible_under_the_read_only_notice() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let original = dir.path().join("Map.ldtk");
    project.save(&original).unwrap();
    let backup = project.backup().unwrap().unwrap().join("Map.ldtk");
    let mut app = EditorApp::empty();
    app.set_project(Project::load(backup).unwrap());
    let before = app.project.as_ref().unwrap().json.clone();
    for panel in [Panel::Project, Panel::Layers, Panel::Help] {
        app.show_panel(Some(panel));
        let mut out = egui::FullOutput::default();
        for pressed in [true, false] {
            let pos = egui::pos2(294., 240.);
            out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.))),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                    ],
                    ..Default::default()
                },
                |ctx| { app.source_panel(ctx, panel); },
            );
        }
        let label = |name| out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == name));
        assert_eq!(label("This panel is disabled for backup files."), panel != Panel::Help);
        if panel == Panel::Project {
            assert!(label("Backup on save"));
            assert!(label("RESTORE THIS BACKUP"));
        }
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
}

#[test]
fn modal_panel_uses_the_source_canvas_mask_and_keeps_recovery_above_it() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|style| style.animation_time = 0.);
    let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
    let mut app = EditorApp::empty();
    app.set_project(Project::new());
    let draw = |app: &mut EditorApp, panel| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                app.canvas_notifications(ctx, screen);
                assert!(app.source_panel(ctx, panel));
                ctx.move_to_top(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("source-definition-panel"),
                ));
            },
        )
    };
    draw(&mut app, Panel::Level);
    app.last_chance = Some(("Level resized".into(), Instant::now()));
    app.report_resize_removals(2);
    draw(&mut app, Panel::Level);
    let output = draw(&mut app, Panel::Level);
    let mask = |out: &egui::FullOutput| {
        out.shapes.iter().enumerate().find_map(|(index, shape)| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.rect.right_bottom() == screen.right_bottom()
                    && rect.fill.r() == 0
                    && (1..255).contains(&rect.fill.a()) =>
            {
                Some((index, rect.rect, rect.fill))
            }
            _ => None,
        })
    };
    let (shade, rect, fill) = mask(&output).expect("modal panel canvas mask");
    assert_eq!(rect, Rect::from_min_max(egui::pos2(300., 0.), screen.max));
    assert_eq!(fill, Color32::from_black_alpha(89));
    for label in ["Level resized", "2 entity(ies) deleted during resizing!"] {
        let notice = output
            .shapes
            .iter()
            .position(|shape| text_center(&shape.shape, label).is_some())
            .expect(label);
        assert!(notice > shade, "{label} must paint above the mask");
    }
    assert!(mask(&draw(&mut app, Panel::World)).is_none());
    assert!(mask(&draw(&mut app, Panel::Rules)).is_none());
}

#[test]
fn header_controls_do_not_change_the_panel_body_position() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            for (panel, height) in
                [(Panel::Layers, 53.), (Panel::Rules, 46.), (Panel::Help, 53.)]
            {
                for padding in [0., 6.] {
                    ui.spacing_mut().button_padding.y = padding;
                    let top = ui.next_widget_position().y;
                    assert!(!source_panel_header(ui, panel, 400., height, title(panel), false));
                    assert_eq!(ui.next_widget_position().y, top + height, "{panel:?}");
                }
            }
        });
    });
}

#[test]
fn tileset_sidebar_uses_compact_rows_and_orders_untagged_before_named_groups() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|style| style.animation_time = 0.);
    let mut app = EditorApp::empty();
    app.set_project(Project::new());
    let draw = |app: &mut EditorApp| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| assert!(app.source_panel(ctx, Panel::Tilesets)),
        )
    };
    let center = |out: &egui::FullOutput, label| {
        out.shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, label))
    };
    draw(&mut app);
    let empty = draw(&mut app);
    assert!(center(&empty, "No tileset").is_some());
    let library_help = "Tilesets are large images in which you pick sub-parts to draw them in your levels. They can also be used as icons for some specific elements (like Enums).";
    assert!(center(&empty, library_help).is_some());
    let builtin = app.project.as_mut().unwrap().ensure_embedded_icons().unwrap();
    app.definition_uid = Some(builtin);
    let single = draw(&mut app);
    assert!(center(&single, library_help).is_none());
    assert!(center(&single, "TILESETS").is_none());
    let row = single.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(row) if row.fill == theme::ACCENT && row.rect.width() > 200. => {
            Some(row.rect)
        }
        _ => None,
    }).expect("selected tileset row");
    assert_eq!(row.x_range(), egui::Rangef::new(29., 311.));
    assert!((row.top() - 198.).abs() <= 1.);
    assert!((row.height() - 33.).abs() <= 1.);
    let project = app.project.as_mut().unwrap();
    for (image, tag) in [("Canopy.png", "Forest"), ("Road.png", "city")] {
        let uid = project.add_tileset(Path::new(image), 16, 32, 32).unwrap();
        project.update_tileset_definition(uid, &json!({"tags":[tag]})).unwrap();
    }
    let tagged = draw(&mut app);
    let headings = ["UNTAGGED", "CITY", "FOREST"].map(|name| {
        center(&tagged, name).unwrap_or_else(|| panic!("missing group {name}")).y
    });
    assert!(headings[0] < headings[1] && headings[1] < headings[2]);
}

#[test]
fn project_and_level_settings_scroll_the_header_with_the_form() {
    for (panel, width) in [(Panel::Project, 840.), (Panel::Level, 490.)] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let mut time = 0.;
        let mut frame = |open, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 500.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    if open {
                        assert!(app.source_panel(ctx, panel));
                    }
                },
            )
        };
        let header = |output: &egui::FullOutput| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == theme::LIGHT
                        && rect.rect.width() >= 400.
                        && rect.rect.height() == 53. =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
        };
        frame(true, vec![]);
        let initial = header(&frame(true, vec![])).expect("settings panel header");
        assert_eq!(initial.top(), 34.);
        frame(
            true,
            vec![
                egui::Event::PointerMoved(egui::pos2(345., 330.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -390.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..5 {
            frame(true, vec![]);
        }
        let scrolled = frame(true, vec![]);
        assert!(
            header(&scrolled).is_none_or(|rect| rect.top() < 20.),
            "{panel:?}: title stayed pinned"
        );
        frame(false, vec![]);
        let reopened = frame(true, vec![]);
        let reopened = header(&reopened).expect("reopened panel header");
        assert_eq!(reopened.top(), 34.);
        assert_eq!(reopened.width(), width, "{panel:?}");
    }
}

#[test]
fn definition_panels_keep_their_full_width_and_scroll_in_narrow_windows() {
    for (panel, width) in [
        (Panel::Layers, 1000.),
        (Panel::Entities, 1000.),
        (Panel::Enums, 840.),
        (Panel::Tilesets, 1000.),
        (Panel::Fields, 790.),
        (Panel::World, 400.),
    ] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.add_entity_def("Marker", 16, 16).unwrap();
        project.add_enum_definition("Kind", &[]).unwrap();
        app.set_project(project);
        let mut time = 0.;
        let mut draw = |window_width, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(window_width, 600.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, panel)),
            )
        };
        let header = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == theme::LIGHT
                            && rect.rect.top() == 34.
                            && rect.rect.height() == 53.
                            && rect.rect.width() > 300. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("definition panel header")
        };
        let narrow_width = match panel {
            Panel::Fields => 720.,
            Panel::World => 340.,
            _ => 800.,
        };
        for window_width in [1440., narrow_width] {
            draw(window_width, vec![]);
            draw(window_width, vec![]);
            let initial = header(&draw(window_width, vec![]));
            assert_eq!(initial.x_range(), egui::Rangef::new(0., width), "{panel:?}");
        }
        draw(
            narrow_width,
            vec![
                egui::Event::PointerMoved(egui::pos2(narrow_width - 60., 295.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-500., 0.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..6 {
            draw(narrow_width, vec![]);
        }
        let scrolled = header(&draw(narrow_width, vec![]));
        assert!(
            scrolled.left() < -30. && scrolled.right() <= narrow_width + 1.,
            "{panel:?}: {scrolled:?}"
        );
    }
}

#[test]
fn source_layer_form_keeps_the_outer_scrollbar_out_of_the_column_grid() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|style| style.animation_time = 0.);
    let mut app = EditorApp::empty();
    let mut project = Project::new();
    project.add_layer("Ground", "IntGrid", 16).unwrap();
    app.set_project(project);
    let mut time = 0.;
    let mut draw = |visible, events| {
        time += 0.05;
        ctx.run(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 620.))),
                events,
                ..Default::default()
            },
            |ctx| {
                if visible {
                    assert!(app.source_panel(ctx, Panel::Layers));
                }
            },
        )
    };
    draw(true, vec![]);
    let initial = draw(true, vec![]);
    let form = initial.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.fill == GROUP_BAR && rect.rect.top() == 104. => {
            Some(rect.rect)
        }
        _ => None,
    });
    assert_eq!(form.unwrap().x_range(), egui::Rangef::new(287., 979.));
    let title_visible = |output: &egui::FullOutput| {
        output.shapes.iter().any(|shape| {
            text_center(&shape.shape, "PROJECT LAYERS")
                .is_some_and(|center| center.y >= 34. && shape.clip_rect.contains(center))
        })
    };
    assert!(title_visible(&initial));
    draw(true, vec![egui::Event::PointerMoved(egui::pos2(750., 305.))]);
    draw(true, vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0., -360.),
        modifiers: egui::Modifiers::NONE,
    }]);
    for _ in 0..5 {
        draw(true, vec![]);
    }
    assert!(!title_visible(&draw(true, vec![])));
    draw(false, vec![]);
    draw(true, vec![]);
    assert!(title_visible(&draw(true, vec![])));
}

#[test]
fn source_layer_sidebar_keeps_full_width_compact_rows_and_selects_by_icon() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|style| style.animation_time = 0.);
    let mut project = Project::new();
    let lower = project.add_layer("Road", "Tiles", 16).unwrap();
    let upper = project.add_layer("Landscape_Collision", "IntGrid", 16).unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    let active_layer = |app: &EditorApp| {
        app.project
            .as_ref()
            .unwrap()
            .layer(app.selected.unwrap(), app.layer)
            .unwrap()["layerDefUid"]
            .as_i64()
            .unwrap()
    };
    assert_eq!(active_layer(&app), upper);
    app.definition_uid = Some(lower);
    app.show_panel(Some(Panel::Layers));
    assert_eq!(app.definition_uid, Some(upper));
    let mut time = 0.;
    let mut draw = |app: &mut EditorApp, events| {
        time += 0.05;
        ctx.run(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.))),
                events,
                ..Default::default()
            },
            |ctx| assert!(app.source_panel(ctx, Panel::Layers)),
        )
    };
    draw(&mut app, vec![]);
    let output = draw(&mut app, vec![]);
    let selected = output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.fill == theme::ACCENT && rect.rect.width() > 100. => {
            Some(rect.rect)
        }
        _ => None,
    });
    assert_eq!(
        selected,
        Some(Rect::from_min_size(egui::pos2(25., 194.), egui::vec2(240., 34.)))
    );
    let list = output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) if rect.fill == theme::DARK && rect.rect.height() > 500. => {
            Some(rect.rect)
        }
        _ => None,
    });
    let list = list.expect("full-height layer list");
    assert_eq!(list.x_range(), egui::Rangef::new(20., 270.));
    assert!((list.top() - 189.).abs() < 0.5, "{list:?}");
    assert!(list.bottom() >= 875.5, "{list:?}");

    let pos = egui::pos2(65., 246.);
    for pressed in [true, false] {
        draw(&mut app, vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
    }
    assert_eq!(app.definition_uid, Some(lower));
    assert_eq!(active_layer(&app), lower);
    app.source_reorder_definition(&ctx, "layers", upper, 1);
    assert_eq!(app.definition_uid, Some(upper));
    assert_eq!(app.definition_index("layers", upper), 1);
    assert_eq!(active_layer(&app), upper);
    app.source_create_definition("layers", None);
    assert_eq!(Some(active_layer(&app)), app.definition_uid);
    assert_ne!(active_layer(&app), upper);
}

fn draw_section(
    ctx: &egui::Context,
    open: &mut bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640., 480.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(360.);
                *open = section(ui, "WORLD SETTINGS: \"World\"", *open, |ui| {
                    ui.label("Section content");
                });
            });
        },
    )
}
#[test]
fn source_collapser_hover_highlights_the_header_and_group_border_and_changes_cursor() {
    for (mut open, cursor) in [
        (true, egui::CursorIcon::ZoomOut),
        (false, egui::CursorIcon::Cell),
    ] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = draw_section(&ctx, &mut open, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "WORLD SETTINGS: \"World\""))
            .unwrap();
        draw_section(&ctx, &mut open, vec![egui::Event::PointerMoved(pos)]);
        let output = draw_section(&ctx, &mut open, vec![]);
        assert_eq!(output.platform_output.cursor_icon, cursor);
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && rect.fill==Color32::from_rgb(255,204,0))),"hovered title must be yellow");
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && rect.stroke.color==Color32::from_rgb(255,204,0) && (!open || rect.rect.height()>28.))),"hovered expanded group must have a yellow content border");
        let output = draw_section(
            &ctx,
            &mut open,
            vec![egui::Event::PointerMoved(egui::pos2(620., 450.))],
        );
        assert_eq!(
            output.platform_output.cursor_icon,
            egui::CursorIcon::Default
        );
        assert!(!output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && (rect.fill==Color32::from_rgb(255,204,0) || rect.stroke.color==Color32::from_rgb(255,204,0)))));
    }
}

#[test]
fn source_collapser_click_switches_the_hover_cursor_and_content_visibility() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut open = true;
    let output = draw_section(&ctx, &mut open, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| text_center(&shape.shape, "WORLD SETTINGS: \"World\""))
        .unwrap();
    for expected_open in [false, true] {
        draw_section(&ctx, &mut open, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            draw_section(
                &ctx,
                &mut open,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
        let output = draw_section(&ctx, &mut open, vec![]);
        assert_eq!(open, expected_open);
        assert_eq!(
            output.platform_output.cursor_icon,
            if open {
                egui::CursorIcon::ZoomOut
            } else {
                egui::CursorIcon::Cell
            }
        );
        assert_eq!(
            output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, "Section content").is_some()),
            open
        );
    }
}
#[test]
fn world_level_operations_follow_the_source_button_strip() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut app = EditorApp::empty();
    app.set_project(Project::new());
    let mut draw = |time| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 800.),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ctx| assert!(app.source_panel(ctx, Panel::World)),
        )
    };
    draw(0.);
    draw(1.);
    let output = draw(2.);
    let rects = output.shapes.iter().filter_map(|shape| match &shape.shape {
        egui::Shape::Rect(rect) => Some(rect),
        _ => None,
    });

    let button_rects = rects
        .clone()
        .filter(|shape| {
            let [r, g, b, _] = shape.fill.to_array();
            r > 100
                && u16::from(r) > u16::from(g) * 2
                && b < 50
                && shape.rect.top() < 111.
                && shape.rect.height() >= 40.
        })
        .map(|shape| (shape.rect.min, shape.rect.size()))
        .collect::<Vec<_>>();
    let expected = [
        (egui::pos2(17., 106.), egui::vec2(266.69, 42.66)),
        (egui::pos2(285.69, 106.), egui::vec2(42.66, 42.66)),
        (egui::pos2(330.34, 106.), egui::vec2(42.66, 42.66)),
    ];
    assert_eq!(button_rects.len(), expected.len());
    for (&(position, size), (source_position, source_size)) in button_rects.iter().zip(expected) {
        assert!((position - source_position).abs().max_elem() <= 1.);
        assert!((size - source_size).abs().max_elem() <= 1.);
    }
    assert!(
        rects.clone().any(|shape| {
            shape.fill == GROUP_BAR
                && (shape.rect.min - egui::pos2(17., 166.)).abs().max_elem() <= 1.
        }),
        "World settings starts after the source 16 point gutter"
    );
}

#[test]
fn multi_world_panel_uses_the_source_action_rows() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut app = EditorApp::empty();
    let mut project = Project::new();
    project.add_world("Away").unwrap();
    app.set_project(project);
    let mut draw = |time| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ctx| assert!(app.source_panel(ctx, Panel::World)),
        )
    };
    draw(0.);
    draw(1.);
    let output = draw(2.);
    let rects: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => Some((rect.fill, rect.rect)),
            _ => None,
        })
        .collect();
    for (color, x, y, width, height) in [
        (theme::LIGHT, 16., 105., 269., 45.),
        (Color32::from_rgb(227, 77, 0), 286., 106., 43., 43.),
        (Color32::from_rgb(121, 17, 5), 330., 106., 43., 43.),
        (Color32::from_rgb(227, 77, 0), 17., 159., 266., 43.),
        (GROUP_BAR, 17., 218., 356., 28.),
    ] {
        assert!(
            rects.iter().any(|(fill, rect)| {
                *fill == color
                    && (rect.min - egui::pos2(x, y)).abs().max_elem() <= 1.
                    && (rect.size() - egui::vec2(width, height)).abs().max_elem() <= 1.
            }),
            "missing action or heading at ({x}, {y}) with size ({width}, {height}); same-color controls: {:?}",
            rects.iter().filter(|(fill, _)| *fill == color).collect::<Vec<_>>()
        );
    }
}

#[test]
fn world_selector_escape_keeps_the_panel_and_keyboard_selects_the_next_world() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut app = EditorApp::empty();
    let mut project = Project::new();
    let away = project.add_world("Away").unwrap();
    let target = project.add_level(Some(away), "Other", 256, 256).unwrap();
    app.set_project(project);
    app.repaint = Some(ctx.clone());
    app.show_panel(Some(Panel::World));
    let draw = |app: &mut EditorApp, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                if app.panel == Some(Panel::World) {
                    app.source_panel(ctx, Panel::World);
                }
            },
        )
    };
    let click = |app: &mut EditorApp, pos| {
        draw(app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            draw(
                app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        draw(app, vec![])
    };
    let open = |app: &mut EditorApp| {
        let output = click(app, egui::pos2(220., 125.));
        assert!(egui::Popup::is_any_open(&ctx));
        output
    };
    let press = |app: &mut EditorApp, key| {
        for pressed in [true, false] {
            draw(
                app,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    };
    draw(&mut app, vec![]);
    let original = app.selected;
    open(&mut app);
    for _ in 0..12 {
        draw(&mut app, vec![]);
    }
    let opened = draw(&mut app, vec![]);
    let active_row = opened
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(row) if row.fill == source_controls::FORM_OPTION_HIGHLIGHT => {
                Some(row.rect)
            }
            _ => None,
        })
        .expect("highlighted world option");
    assert!((active_row.height() - 17.).abs() <= 1.);
    assert!((active_row.width() - 267.).abs() <= 2., "{active_row:?}");
    press(&mut app, Key::Escape);
    assert_eq!(app.panel, Some(Panel::World));
    assert!(app.world);
    assert_eq!(app.selected, original);
    assert!(!egui::Popup::is_any_open(&ctx));

    open(&mut app);
    press(&mut app, Key::ArrowDown);
    assert_eq!(app.selected, original);
    press(&mut app, Key::Enter);
    assert_eq!(app.selected, Some(target));
    assert_eq!(app.active_world, Some(away));
    assert!(!app.world);
    assert_eq!(app.panel, None);
    assert!(!egui::Popup::is_any_open(&ctx));
    app.show_panel(Some(Panel::World));
    open(&mut app);
    click(&mut app, active_row.center());
    assert_eq!(app.selected, original);
    assert_eq!(app.panel, None);
    assert!(!app.world);
}

#[test]
fn world_and_level_forms_anchor_controls_and_reserve_the_world_scrollbar() {
    for (panel, xs) in [
        (Panel::World, &[143., 135.][..]),
        (Panel::Level, &[139.][..]),
    ] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let mut draw = |time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 800.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, panel)),
            )
        };
        draw(0.);
        draw(1.);
        let output = draw(2.);
        let rects = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(shape) => Some(shape),
            _ => None,
        });
        for x in xs {
            assert!(
                rects.clone().any(|shape| {
                    shape.rect.left() == *x
                        && shape.rect.width() == 60.
                        && shape.rect.height() == 33.
                }),
                "missing source-sized numeric box at x={x}"
            );
        }
        if panel == Panel::World {
            let headers = rects
                .filter(|shape| shape.fill == GROUP_BAR && shape.rect.height() == 28.)
                .map(|shape| (shape.rect.left(), shape.rect.right()))
                .collect::<Vec<_>>();
            assert_eq!(
                headers,
                [(17., 373.); 3],
                "World forms reserve the 10 point scrollbar"
            );
        } else {
            assert!(
                rects.clone().any(|shape| {
                    shape.fill == GROUP_BAR && shape.rect.min == egui::pos2(21., 112.)
                }),
                "Current level keeps the source gutter below its header"
            );
        }
    }
}

fn draw_fields(
    app: &mut EditorApp,
    ctx: &egui::Context,
    owner: i64,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 900.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(200.);
                app.source_field_list(ui, Some(owner), 400.);
            });
            app.source_field_type_picker(ctx);
        },
    )
}
fn text_center(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
    match shape {
        egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
            Some(text.pos + text.galley.rect.center().to_vec2())
        }
        egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_center(s, label)),
        _ => None,
    }
}
fn click_field(app: &mut EditorApp, ctx: &egui::Context, owner: i64, pos: egui::Pos2) {
    for pressed in [true, false] {
        draw_fields(
            app,
            ctx,
            owner,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
fn draw_first_field_definition(
    app: &mut EditorApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let project = app.project.as_ref().unwrap();
    let def = app
        .field_owner
        .and_then(|owner| {
            project.json["defs"]["entities"]
                .as_array()?
                .iter()
                .find(|entity| entity["uid"] == owner)
        })
        .map_or(&project.json["defs"]["levelFields"], |owner| &owner["fieldDefs"])[0]
        .clone();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 900.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.source_field_properties(ui, &def);
            });
        },
    )
}

#[test]
fn array_length_inputs_share_a_row_and_move_the_opposite_bound() {
    let mut project = Project::new();
    let uid = project
        .add_field_definition(None, "Entries", "Int", true)
        .unwrap();
    project
        .update_field_definition(uid, &json!({"arrayMaxLength":4}))
        .unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let output = draw_first_field_definition(&mut app, &ctx, vec![]);
    let position = |label| {
        output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, label))
            .unwrap_or_else(|| panic!("missing {label}"))
    };
    let min = position("min length");
    let max = position("4");
    assert!((min.y - max.y).abs() <= 1.);
    assert!(min.x < max.x);
    for pressed in [true, false] {
        draw_first_field_definition(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(min),
                egui::Event::PointerButton {
                    pos: min,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    draw_first_field_definition(&mut app, &ctx, vec![egui::Event::Text("7".into())]);
    draw_first_field_definition(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
    assert_eq!(def["arrayMinLength"], 7);
    assert_eq!(def["arrayMaxLength"], 7);
}

#[test]
fn bool_array_definition_renders_and_edits_the_saved_default_state() {
    let mut project = Project::new();
    let uid = project
        .add_field_definition(None, "Enabled", "Bool", true)
        .unwrap();
    project.set_field_default(uid, json!(true)).unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let output = draw_first_field_definition(&mut app, &ctx, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(0, 117, 255) => {
                Some(rect.rect.center())
            }
            _ => None,
        })
        .expect("checked array default");
    for pressed in [true, false] {
        draw_first_field_definition(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
    assert_eq!(
        def["defaultOverride"],
        json!({"id":"V_Bool","params":[false]})
    );
}

#[test]
fn numeric_field_can_save_zero_as_an_explicit_default() {
    let mut project = Project::new();
    project
        .add_field_definition(None, "Count", "Int", false)
        .unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let output = draw_first_field_definition(&mut app, &ctx, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| text_center(&shape.shape, "0"))
        .expect("zero default");
    for pressed in [true, false] {
        draw_first_field_definition(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    draw_first_field_definition(&mut app, &ctx, vec![egui::Event::Text("0".into())]);
    draw_first_field_definition(
        &mut app,
        &ctx,
        vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
    assert_eq!(def["defaultOverride"], json!({"id":"V_Int","params":[0]}));
}

#[test]
fn reference_policy_resets_symmetry_and_initializes_a_specific_target() {
    let mut project = Project::new();
    let owner = project.add_entity_def("Door", 16, 16).unwrap();
    let uid = project
        .add_field_definition(Some(owner), "Pair", "EntityRef", false)
        .unwrap();
    project
        .update_field_definition(uid, &json!({"symmetricalRef":true}))
        .unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    app.field_owner = Some(owner);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let click = |app: &mut EditorApp, label| {
        let output = draw_first_field_definition(app, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, label))
            .unwrap_or_else(|| panic!("missing {label}"));
        for pressed in [true, false] {
            draw_first_field_definition(
                app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    };
    click(&mut app, "Only another 'Door's");
    draw_first_field_definition(
        &mut app,
        &ctx,
        [Key::ArrowDown, Key::ArrowDown, Key::Enter]
            .into_iter()
            .map(|key| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            })
            .collect(),
    );
    let def = |app: &EditorApp| {
        app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0].clone()
    };
    assert_eq!(def(&app)["allowedRefs"], "OnlySpecificEntity");
    assert_eq!(def(&app)["allowedRefsEntityUid"], owner);
    assert_eq!(def(&app)["symmetricalRef"], true);
    click(&mut app, "Only a specific Entity");
    click(&mut app, "Any entity");
    assert_eq!(def(&app)["allowedRefs"], "Any");
    assert_eq!(def(&app)["symmetricalRef"], false);
}

#[test]
fn custom_field_world_view_checkbox_is_visible_only_for_level_fields() {
    let mut project = Project::new();
    let entity = project.add_entity_def("Actor", 16, 16).unwrap();
    for owner in [None, Some(entity)] {
        project
            .add_field_definition(owner, "Caption", "String", false)
            .unwrap();
    }
    let mut app = EditorApp::empty();
    app.set_project(project);
    for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
        app.field_owner = owner;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut draw = |time| {
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 1200.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    assert!(app.source_panel(ctx, panel));
                },
            )
        };
        draw(0.);
        let output = draw(1.);
        let has_label = |label| {
            output
                .shapes
                .iter()
                .any(|shape| text_center(&shape.shape, label).is_some())
        };
        assert!(has_label("Always show"), "field form must be visible");
        assert_eq!(has_label("Show in world view"), owner.is_none());
    }
}

#[test]
fn field_display_controls_follow_the_owner_and_mode_in_one_row() {
    let mut project = Project::new();
    let entity = project.add_entity_def("Actor", 16, 16).unwrap();
    let health = project
        .add_field_definition(None, "Health", "Int", false)
        .unwrap();
    project.set_field_default(health, json!(5)).unwrap();
    project
        .update_field_definition(
            health,
            &json!({"editorDisplayMode":"NameAndValue","editorDisplayScale":1.25,
                "editorShowInWorld":true,"editorDisplayPos":"Beneath"}),
        )
        .unwrap();
    let path = project
        .add_field_definition(Some(entity), "Path", "Point", true)
        .unwrap();
    project
        .update_field_definition(
            path,
            &json!({"editorDisplayMode":"PointPath","editorLinkStyle":"CurvedArrow",
                "editorDisplayScale":1.25,"editorDisplayPos":"Beneath"}),
        )
        .unwrap();
    for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.field_owner = owner;
        app.settings.set_ui_state_int("extraEntityDetails", None, 0);
        let draw = |app: &mut EditorApp, time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, panel)),
            )
        };
        draw(&mut app, 0.);
        let output = draw(&mut app, 1.);
        let at = |label| {
            output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
        };
        assert!(at("Beneath").is_none(), "unavailable text position");
        if owner.is_none() {
            let world = at("Show in world view").unwrap();
            assert!(world.y < 500., "Display row is too low: {}", world.y);
            let scale = at("125").expect("scale displayed as a percentage");
            assert!((world.y - scale.y).abs() <= 2.);
            assert!(at("1234").is_some(), "paired prefix and suffix preview");
        } else {
            let mode = at("Show path of points").unwrap();
            let style = at("Curved arrow").unwrap();
            assert!((mode.y - style.y).abs() <= 2.);
            assert!(at("125").is_none(), "graphical modes have no text scale");
            assert!(at("Show in world view").is_none());
        }
    }
}

#[test]
fn entity_field_display_color_stays_on_the_same_row_as_scale() {
    let mut project = Project::new();
    let owner = project.add_entity_def("Actor", 16, 16).unwrap();
    let uid = project
        .add_field_definition(Some(owner), "Costs", "Int", true)
        .unwrap();
    project
        .update_field_definition(
            uid,
            &json!({
                "editorDisplayMode":"NameAndValue",
                "editorDisplayScale":1.25,
                "editorDisplayColor":"#0135AB",
            }),
        )
        .unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    app.settings.set_ui_state_int("extraEntityDetails", None, 0);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let draw = |app: &mut EditorApp, time| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ctx| assert!(app.source_panel(ctx, Panel::Entities)),
        )
    };
    draw(&mut app, 0.);
    let output = draw(&mut app, 1.);
    let scale = output
        .shapes
        .iter()
        .find_map(|shape| text_center(&shape.shape, "125"))
        .expect("display scale");
    let color = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(1, 53, 171) => {
                Some(rect.rect.center())
            }
            _ => None,
        })
        .expect("display color");
    assert!(
        (scale.y - color.y).abs() <= 2.,
        "scale {scale:?}, color {color:?}"
    );
}

#[test]
fn level_point_picker_keeps_the_form_open_through_cancel_and_canvas_commit() {
    let mut project = Project::new();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let level = project.level_keys()[0];
    let uid = project
        .add_field_definition(None, "Target", "Point", false)
        .unwrap();
    let original = json!({"cx":10,"cy":8});
    project
        .set_field_editor_value(level, None, uid, original.clone())
        .unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    app.show_panel(Some(Panel::Level));
    app.fit = false;
    app.viewport.center = [128., 128.];
    app.viewport.zoom = 1.;
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
    let draw = |app: &mut EditorApp, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                let level_point_picker_was_open = app.picking_point_in_level_panel();
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                    });
                app.dialogs(ctx, level_point_picker_was_open);
            },
        )
    };
    let click = |app: &mut EditorApp, pos| {
        draw(app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            draw(
                app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
    };
    let control = |output: &egui::FullOutput, label| {
        output.shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, label))
            .unwrap()
    };
    let value = |app: &EditorApp| {
        app.project.as_ref().unwrap().level(level).unwrap()["fieldInstances"][0]["__value"]
            .clone()
    };
    draw(&mut app, vec![]);
    let pick = control(&draw(&mut app, vec![]), "10,8");
    click(&mut app, pick);
    assert!(app.picking_point());
    assert_eq!(app.panel, Some(Panel::Level));
    let cancel = control(&draw(&mut app, vec![]), "CANCEL");
    click(&mut app, cancel);
    assert!(!app.picking_point());
    assert_eq!(value(&app), original);

    click(&mut app, pick);
    let point = app
        .viewport
        .to_screen([104., 120.], screen.min.into(), screen.size().into());
    click(&mut app, point.into());
    assert_eq!(value(&app), json!({"cx":6,"cy":7}));
    assert!(!app.picking_point());
    assert_eq!(app.panel, Some(Panel::Level));
    let updated = control(&draw(&mut app, vec![]), "6,7");
    click(&mut app, updated);
    click(&mut app, egui::pos2(450., 78.));
    assert!(!app.picking_point());
    assert!(app.panel.is_none());
}

#[test]
fn level_field_panel_returns_to_current_level_when_dismissed() {
    let mut app = EditorApp::empty();
    app.set_project(Project::new());
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let draw = |app: &mut EditorApp, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                if let Some(panel) = app.panel {
                    app.source_panel(ctx, panel);
                }
            },
        )
    };
    app.show_panel(Some(Panel::Fields));
    draw(&mut app, Vec::new());
    let close = egui::pos2(750., 78.);
    draw(&mut app, vec![egui::Event::PointerMoved(close)]);
    for pressed in [true, false] {
        draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(close),
                egui::Event::PointerButton {
                    pos: close,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert_eq!(app.panel, Some(Panel::Level));

    app.show_panel(Some(Panel::Fields));
    draw(
        &mut app,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert_eq!(app.panel, Some(Panel::Level));
}

#[test]
fn entity_and_level_field_definitions_share_one_control_column() {
    fn text_x(shape: &egui::Shape, label: &str) -> Option<f32> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => Some(text.pos.x),
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .filter_map(|shape| text_x(shape, label))
                .max_by(f32::total_cmp),
            _ => None,
        }
    }

    let mut project = Project::new();
    let entity = project.add_entity_def("Actor", 16, 16).unwrap();
    for owner in [None, Some(entity)] {
        let field = project
            .add_field_definition(owner, "Caption", "String", false)
            .unwrap();
        project
            .update_field_definition(
                field,
                &json!({"doc":"Example documentation","canBeNull":false}),
            )
            .unwrap();
        project
            .set_field_default(field, json!("Original caption"))
            .unwrap();
    }
    for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.field_owner = owner;
        app.settings.set_ui_state_int("extraEntityDetails", None, 0);
        let mut draw = |time| {
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, panel)),
            )
        };
        draw(0.);
        draw(1.);
        let output = draw(2.);
        let column = |label| {
            output
                .shapes
                .iter()
                .filter_map(|item| text_x(&item.shape, label))
                .max_by(f32::total_cmp)
                .unwrap_or_else(|| panic!("{label} is missing in {}", title(panel)))
        };
        if owner.is_none() {
            assert!(
                output.shapes.iter().any(|item| {
                    matches!(&item.shape, egui::Shape::Rect(shape) if
                        shape.fill == theme::LIGHT
                        && shape.rect == Rect::from_min_size(egui::pos2(0., 34.), egui::vec2(790., 53.))
                    )
                }),
                "level field definitions use a 750-point body with 20-point side padding"
            );
            let form_x = column("GENERAL SETTINGS");
            assert!(
                (form_x - 228.).abs() <= 1.,
                "field properties start at x={form_x}, expected x=228"
            );
        }
        let identifier = column("Caption");
        for control in ["Example documentation", "Original caption"] {
            let x = column(control);
            assert!(
                (x - identifier).abs() <= 1.,
                "{control} starts at {x}, but the field identifier starts at {identifier} in {}",
                title(panel)
            );
        }
    }
}

#[test]
fn file_extension_editor_preserves_source_array_schema_and_normalization() {
    let mut p = Project::new();
    let uid = p
        .add_field_definition(None, "Asset", "FilePath", false)
        .unwrap();
    let types = source_file_types(" .PNG, jpg; PNG ..bad ...bad __ * .tar.gz ");
    assert_eq!(types, json!([".png", ".jpg", "..bad", ".tar.gz"]));
    p.update_field_definition(uid, &json!({"acceptFileTypes":types}))
        .unwrap();
    assert_eq!(
        p.json["defs"]["levelFields"][0]["acceptFileTypes"],
        json!([".png", ".jpg", "..bad", ".tar.gz"])
    );
    assert_eq!(source_file_types("???"), Value::Null);
    assert_eq!(source_file_types("__ ..."), json!([]));
}
#[test]
fn native_add_field_chooses_type_before_mutating_and_can_undo() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut app = EditorApp::empty();
    let mut p = Project::new();
    let owner = p.add_entity_def("Actor", 16, 16).unwrap();
    app.set_project(p);
    let before = app.project.as_ref().unwrap().json.clone();
    let output = draw_fields(&mut app, &ctx, owner, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|s| text_center(&s.shape, "+ SINGLE VALUE"))
        .unwrap();
    click_field(&mut app, &ctx, owner, pos);
    assert_eq!(
        app.project.as_ref().unwrap().json,
        before,
        "opening type picker must not create an Int field"
    );
    draw_fields(&mut app, &ctx, owner, vec![]);
    let output = draw_fields(&mut app, &ctx, owner, vec![]);
    let pos = output
        .shapes
        .iter()
        .find_map(|s| text_center(&s.shape, "Float"))
        .expect("source type chooser");
    click_field(&mut app, &ctx, owner, pos);
    assert_eq!(
        app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0]["__type"],
        "Float"
    );
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(app.project.as_ref().unwrap().json, before);
}
#[test]
fn sparse_saved_selection_ids_do_not_expand_to_the_bounding_rectangle() {
    let mut app = EditorApp::empty();
    let mut p = Project::new();
    p.json["defs"]["tilesets"] =
        json!([{"uid":10,"tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4}]);
    app.set_project(p);
    app.tile_rect = [0, 0, 3, 3];
    app.atlas_selection = Some((10, vec![0, 2, 8, 10]));
    assert_eq!(app.selected_tile_ids_for(10), vec![0, 2, 8, 10]);
    app.atlas_selection = None;
    assert_eq!(
        app.selected_tile_ids_for(10),
        vec![0, 1, 2, 4, 5, 6, 8, 9, 10]
    );
}
#[test]
fn nested_field_form_labels_remain_fully_readable_inside_their_third_column() {
    fn texts<'a>(
        shape: &'a egui::Shape,
        clip: Rect,
        items: &mut Vec<(Rect, &'a egui::epaint::TextShape)>,
    ) {
        match shape {
            egui::Shape::Text(text) => items.push((clip, text)),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, clip, items)
                }
            }
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut project = Project::load(resource_path("examples/original/Entities.ldtk")).unwrap();
    project.migrate_to_current().unwrap();
    let mut app = EditorApp::empty();
    app.set_project(project);
    app.settings.set_ui_state_int("extraEntityDetails", None, 0);
    let mut time = 0.;
    let mut draw = |app: &mut EditorApp| {
        time += 1.;
        ctx.run(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.source_panel(ctx, Panel::Entities);
            },
        )
    };
    draw(&mut app);
    draw(&mut app);
    let output = draw(&mut app);
    let mut items = Vec::new();
    for shape in &output.shapes {
        texts(&shape.shape, shape.clip_rect, &mut items);
    }
    let general = items
        .iter()
        .find(|(_, text)| text.galley.job.text == "GENERAL SETTINGS")
        .unwrap()
        .1;
    let field_text = |label| {
        items
            .iter()
            .find(|(_, text)| {
                text.pos.y > general.pos.y
                    && text
                        .galley
                        .job
                        .text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        == label
            })
            .unwrap_or_else(|| panic!("missing third-column label {label}"))
    };
    for label in ["Field identifier", "Documentation", "Limits", "Can be null"] {
        let (clip, text) = field_text(label);
        let bounds = Rect::from_min_size(text.pos, text.galley.size());
        assert!(
            bounds.left() >= general.pos.x - 0.1,
            "{label} escapes left into the field list: {bounds:?}, third column {}",
            general.pos.x
        );
        assert!(
            clip.expand(0.1).contains_rect(bounds),
            "{label} is clipped instead of readable: text {bounds:?}, clip {clip:?}"
        );
        assert!(text.galley.rows.len() <= 2, "{label} wraps inside words");
        assert!(
            bounds.right() <= general.pos.x + FIELD_LABEL_WIDTH - 32. + 0.1,
            "{label} overlaps its label-cell padding/info pill: {bounds:?}"
        );
        assert!(
            (text.galley.job.sections[0].format.font_id.size - 12.667).abs() < 0.001,
            "source dt labels use 9.5pt"
        );
    }
    let (_, default) = field_text("Default value");
    let (_, limits) = field_text("Limits");
    let (_, nullable) = field_text("Can be null");
    let (_, range_separator) = field_text("to");
    assert!(default.pos.y < limits.pos.y && limits.pos.y < nullable.pos.y);
    assert!((limits.pos.y - range_separator.pos.y).abs() <= 3.);
    let body = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == theme::MEDIUM
                    && rect.rect.left() == 0.
                    && rect.rect.height() > 500. =>
            {
                Some(rect.rect)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        body.right() <= 1000.,
        "third column body must stay under the original 1000-point header: {body:?}"
    );
}
#[test]
fn definition_list_background_requests_clipboard_on_secondary_click() {
    let ctx = egui::Context::default();
    let mut app = EditorApp::empty();
    app.set_project(Project::new());
    app.panel = Some(Panel::Layers);
    let pos = egui::pos2(100., 380.);
    let mut render = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 600.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.source_sidebar(ui, Panel::Layers, LIST_WIDTH, 550.);
                });
            },
        )
    };
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = render(vec![egui::Event::PointerMoved(pos)]);
    let _ = render(vec![button(true)]);
    let output = render(vec![button(false)]);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::RequestPaste)
    );
}

#[test]
fn source_entity_panel_has_original_header_geometry_and_integrated_fields() {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let mut app = EditorApp::empty();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/original/Test_file_for_API_showing_all_features.ldtk");
    let mut project = Project::load(path).unwrap();
    project.migrate_to_current().unwrap();
    app.set_project(project);
    let original = app.project.as_ref().unwrap().json.clone();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            ..Default::default()
        },
        |ctx| {
            app.source_panel(ctx, Panel::Entities);
        },
    );
    let output = ctx.run(
        egui::RawInput {
            time: Some(1.0),
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            ..Default::default()
        },
        |ctx| {
            assert!(app.source_panel(ctx, Panel::Entities));
        },
    );
    let header = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Rect(shape)
                if shape.fill == theme::LIGHT
                    && shape.rect.width() > 900.
                    && shape.rect.height() == 53. =>
            {
                Some(shape.rect)
            }
            _ => None,
        })
        .next()
        .expect("original full-width panel header");
    assert_eq!(
        header,
        Rect::from_min_size(egui::pos2(0., 34.), egui::vec2(1000., 53.))
    );
    let top_border = Rect::from_min_max(header.min, egui::pos2(header.right(), 35.));
    assert!(output.shapes.iter().any(|shape| {
        shape.clip_rect == top_border
            && matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.fill == Color32::from_rgb(115, 127, 157)
                    && rect.rect == header
                    && rect.corner_radius.nw == 0)
    }));
    let body = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::epaint::Shape::Rect(r)
                if r.fill == theme::MEDIUM && r.rect.left() == 0. && r.rect.height() > 500. =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .next()
        .expect("source panel body");
    assert!(
        body.right() <= 1000.,
        "body expanded beyond header: {body:?}"
    );
    let visible_position = |output: &egui::FullOutput, label| {
        output.shapes.iter().find_map(|shape| {
            text_center(&shape.shape, label).filter(|pos| shape.clip_rect.contains(*pos))
        })
    };
    let max_count = visible_position(&output, "Max count").expect("entity settings");
    let add_field = visible_position(&output, "+ ARRAY").expect("field toolbar");
    let heading = visible_position(&output, "GENERAL SETTINGS").expect("field properties");
    let _ = ctx.run(
        egui::RawInput {
            time: Some(1.1),
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            events: vec![
                egui::Event::PointerMoved(heading + egui::vec2(40., 24.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -900.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ctx| {
            app.source_panel(ctx, Panel::Entities);
        },
    );
    let output = ctx.run(
        egui::RawInput {
            time: Some(1.2),
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            ..Default::default()
        },
        |ctx| {
            app.source_panel(ctx, Panel::Entities);
        },
    );
    assert_eq!(visible_position(&output, "Max count"), Some(max_count));
    assert_eq!(visible_position(&output, "+ ARRAY"), Some(add_field));
    assert_ne!(visible_position(&output, "GENERAL SETTINGS"), Some(heading));
    assert_eq!(app.field_owner, app.definition_uid);
    assert_eq!(app.project.as_ref().unwrap().json, original);
}
