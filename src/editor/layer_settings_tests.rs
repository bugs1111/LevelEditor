use super::*;

fn editor(kind: &str) -> (EditorApp, egui::Context, i64) {
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    ctx.style_mut(|s| s.animation_time = 0.);
    let mut p = Project::new();
    let uid = p.add_layer("TestLayer", kind, 16).unwrap();
    p.update_layer_properties(uid, &json!({"displayOpacity":0.37,"inactiveOpacity":0.62,"parallaxFactorX":0.25,"parallaxFactorY":0.25,"parallaxScaling":false})).unwrap();
    let mut app = EditorApp::empty();
    app.set_project(p);
    app.definition_uid = Some(uid);
    app.repaint = Some(ctx.clone());
    (app, ctx, uid)
}
fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 2200.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.properties(ui, Panel::Layers);
            });
        },
    )
}
fn labels(out: &egui::FullOutput, expected: &str) -> Vec<Rect> {
    fn find(s: &egui::Shape, expected: &str, found: &mut Vec<Rect>) {
        match s {
            egui::Shape::Text(t) if t.galley.job.text.replace('\n', " ") == expected => {
                found.push(t.galley.rect.translate(t.pos.to_vec2()))
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    find(s, expected, found);
                }
            }
            _ => {}
        }
    }
    let mut found = vec![];
    for s in &out.shapes {
        find(&s.shape, expected, &mut found);
    }
    found
}
fn label(out: &egui::FullOutput, expected: &str) -> Rect {
    *labels(out, expected)
        .first()
        .unwrap_or_else(|| panic!("missing {expected}"))
}
fn click(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
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
fn input(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2, value: &str) {
    click(app, ctx, pos);
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        app,
        ctx,
        vec![key(
            Key::A,
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..Default::default()
            },
        )],
    );
    let mut events = vec![key(Key::Backspace, Default::default())];
    if !value.is_empty() {
        events.push(egui::Event::Text(value.into()));
    }
    events.push(key(Key::Enter, Default::default()));
    frame(app, ctx, events);
    frame(app, ctx, vec![]);
}
fn layer(app: &EditorApp, uid: i64) -> &Value {
    app.project.as_ref().unwrap().json["defs"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["uid"] == uid)
        .unwrap()
}
#[test]
fn source_layer_groups_and_type_options_do_not_mutate_on_open() {
    for kind in ["Tiles", "IntGrid", "AutoLayer", "Entities"] {
        let (mut app, ctx, _) = editor(kind);
        let before = app.project.as_ref().unwrap().json.clone();
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        assert!(label(&out, "General layer settings").top() < label(&out, "Display options").top());
        label(&out, "Color in layers list");
        label(&out, "UI tags");
        assert_eq!(
            !labels(&out, "Show in world view").is_empty(),
            kind != "Entities"
        );
        assert_eq!(
            !labels(&out, "Allow selections when inactive").is_empty(),
            kind != "AutoLayer"
        );
        assert_eq!(
            !labels(&out, "Hide entity fields when inactive").is_empty(),
            kind == "Entities"
        );
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
}
#[test]
fn source_layer_opacity_percent_clamps_and_is_undoable() {
    let (mut app, ctx, uid) = editor("Tiles");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "37").center(), "0");
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.1);
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.37);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "62").center(), "150");
    assert_eq!(layer(&app, uid)["inactiveOpacity"], 1.);
}
#[test]
fn source_layer_parallax_links_axes_and_scaling_disables_y() {
    let (mut app, ctx, uid) = editor("Tiles");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, labels(&out, "25")[0].center(), "200");
    assert_eq!(layer(&app, uid)["parallaxFactorX"], 1.);
    assert_eq!(layer(&app, uid)["parallaxFactorY"], 1.);
    let out = frame(&mut app, &ctx, vec![]);
    let values = labels(&out, "100");
    input(&mut app, &ctx, values[1].center(), "-50");
    assert_eq!(layer(&app, uid)["parallaxFactorY"], -0.5);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "100").center(), "50");
    assert_eq!(layer(&app, uid)["parallaxFactorY"], -0.5);
    let out = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&out, "Scale ?").center());
    assert_eq!(layer(&app, uid)["parallaxFactorY"], 0.5);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, labels(&out, "50")[1].center(), "-75");
    assert_eq!(layer(&app, uid)["parallaxFactorY"], 0.5);
}
#[test]
fn source_layer_clearing_parallax_y_restores_x() {
    let (mut app, ctx, uid) = editor("Tiles");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(uid, &json!({"parallaxFactorY":-0.5}))
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "-50").center(), "");
    assert_eq!(layer(&app, uid)["parallaxFactorY"], 0.25);
}

#[test]
fn source_layer_percent_edits_round_to_whole_points_and_empty_uses_zero() {
    let (mut app, ctx, uid) = editor("Tiles");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "37").center(), "37.6");
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.38);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "38").center(), "");
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.1);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "62").center(), "invalid");
    assert_eq!(layer(&app, uid)["inactiveOpacity"], 0.);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, labels(&out, "25")[0].center(), "-50.5");
    assert_eq!(layer(&app, uid)["parallaxFactorX"], -0.5);
}

#[test]
fn source_layer_guide_grid_bounds_and_single_pixel_disable_are_applied_on_edit() {
    let (mut app, ctx, uid) = editor("Tiles");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(uid, &json!({"guideGridWid":321,"guideGridHei":17}))
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "321").center(), "1");
    assert_eq!(layer(&app, uid)["guideGridWid"], 0);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "17").center(), "9999");
    assert_eq!(layer(&app, uid)["guideGridHei"], 1024);
}

#[test]
fn source_layer_tile_pivot_only_offers_nine_source_positions() {
    let (mut app, ctx, uid) = editor("Tiles");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    assert!(
        labels(&out, "Advanced…").is_empty(),
        "Tiles must not expose the entity-only advanced pivot editor"
    );
    fn pivot(s: &egui::Shape) -> Option<Rect> {
        match s {
            egui::Shape::Rect(r)
                if (r.rect.width() - 64.).abs() < 0.1 && (r.rect.height() - 64.).abs() < 0.1 =>
            {
                Some(r.rect)
            }
            egui::Shape::Vec(v) => v.iter().find_map(pivot),
            _ => None,
        }
    }
    let rect = out.shapes.iter().find_map(|s| pivot(&s.shape)).unwrap();
    click(&mut app, &ctx, rect.min + egui::vec2(56., 32.));
    assert_eq!(layer(&app, uid)["tilePivotX"], 1.);
    assert_eq!(layer(&app, uid)["tilePivotY"], 0.5);
}
