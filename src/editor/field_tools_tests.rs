use super::*;

struct Fixture {
    app: EditorApp,
    source: LevelKey,
    target: LevelKey,
    owner: String,
    target_iid: String,
    uid: i64,
}
fn fixture(array: bool) -> Fixture {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let actor = p.add_entity_def("Actor", 16, 16).unwrap();
    let uid = p
        .add_field_definition(Some(actor), "Link", "EntityRef", array)
        .unwrap();
    p.update_field_definition(
        uid,
        &json!({"allowedRefs":"OnlySame","allowOutOfLevelRef":true}),
    )
    .unwrap();
    let source = p.level_keys()[0];
    let target = p.add_level(None, "Other", 256, 256).unwrap();
    p.add_entity(source, 0, actor, 32, 32).unwrap();
    p.add_entity(target, 0, actor, 96, 96).unwrap();
    let owner = p.layer(source, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let target_iid = p.layer(target, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut app = EditorApp::empty();
    app.set_project(p);
    app.panel = Some(Panel::Entity);
    app.entity_index = Some(0);
    app.viewport.center = [128., 128.];
    app.viewport.zoom = 1.;
    app.fit = false;
    Fixture {
        app,
        source,
        target,
        owner,
        target_iid,
        uid,
    }
}
fn button(pos: egui::Pos2, pressed: bool, button: egui::PointerButton) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
fn frame(
    f: &mut Fixture,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    controls: bool,
) -> (Rect, Vec<(String, egui::Pos2)>) {
    let mut rect = Rect::NOTHING;
    let mut pick_rect = None;
    let output = ctx.run(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.,400.))), events, ..Default::default() }, |ctx| {
        f.app.shortcuts(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            if controls {
                let entity = f.app.project.as_ref().unwrap().layer(f.source,0).unwrap()["entityInstances"][0].clone();
                let field = entity["fieldInstances"].as_array().unwrap().iter().find(|field|field["defUid"]==f.uid).unwrap().clone();
                let def = f.app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0].clone();
                pick_rect=f.app.field_pick_controls(ui,f.source,Some(&f.owner),&field,&def);
            } else {
                let (r,response) = ui.allocate_exact_size(ui.available_size(),Sense::click_and_drag());
                rect = r;
                f.app.canvas_input(ctx,r,&response);
                f.app.draw_field_pick_targets(ui,r);
            }
        });
    });
    let circles = output
        .shapes
        .iter()
        .filter(|s| matches!(s.shape, egui::Shape::Circle(_)))
        .count();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("target-circles"), circles));
    let mut labels: Vec<(String, egui::Pos2)> = output
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            egui::Shape::Text(t) => {
                Some((t.galley.text().to_owned(), t.pos + t.galley.size() * 0.5))
            }
            _ => None,
        })
        .collect();
    if let Some(pick_rect) = pick_rect {
        labels.push(("Canvas pick".into(), pick_rect.center()));
    }
    (rect, labels)
}
fn click(f: &mut Fixture, ctx: &egui::Context, pos: egui::Pos2, controls: bool) {
    frame(
        f,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            button(pos, true, egui::PointerButton::Primary),
        ],
        controls,
    );
    frame(
        f,
        ctx,
        vec![button(pos, false, egui::PointerButton::Primary)],
        controls,
    );
}
fn start(f: &mut Fixture, ctx: &egui::Context) {
    theme::configure(ctx);
    let (_, labels) = frame(f, ctx, vec![], true);
    let pos = labels.iter().find(|(s, _)| s == "Canvas pick").unwrap().1;
    click(f, ctx, pos, true);
    assert!(f.app.picking_entity_ref());
}

#[test]
fn missing_reference_buttons_use_source_size_and_colors_and_open_the_picker() {
    for (array, height) in [(false, 23.), (true, 22.)] {
        let mut f = fixture(array);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        if array {
            let project = f.app.project.as_mut().unwrap();
            project
                .update_field_definition(f.uid, &json!({"arrayMaxLength":1}))
                .unwrap();
            project
                .edit_field_array(
                    f.source,
                    Some(&f.owner),
                    f.uid,
                    ldtk_core::FieldArrayEdit::Insert { index: 0 },
                )
                .unwrap();
        }
        let draw = |f: &mut Fixture, events| {
            let mut pick_rect = None;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let project = f.app.project.as_ref().unwrap();
                        let def = project.json["defs"]["entities"][0]["fieldDefs"][0].clone();
                        let field = project.layer(f.source, 0).unwrap()["entityInstances"][0]
                            ["fieldInstances"][0]
                            .clone();
                        pick_rect =
                            f.app.field_pick_controls(ui, f.source, Some(&f.owner), &field, &def);
                    });
                },
            );
            (pick_rect.unwrap(), output)
        };
        let (rect, output) = draw(&mut f, vec![]);
        let has_fill = |output: &egui::FullOutput, fill| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(r) if r.rect == rect && r.fill == fill)
            })
        };
        assert_eq!(rect.height(), height);
        assert_eq!(rect.width() < 200., array);
        assert!(has_fill(&output, Color32::from_rgb(121, 17, 5)));
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "MISSING REFERENCE")
        }));
        let pos = rect.center();
        let (_, output) = draw(&mut f, vec![egui::Event::PointerMoved(pos)]);
        assert!(has_fill(&output, Color32::from_rgb(157, 18, 2)));
        draw(&mut f, vec![button(pos, true, egui::PointerButton::Primary)]);
        draw(&mut f, vec![button(pos, false, egui::PointerButton::Primary)]);
        assert!(f.app.picking_entity_ref());
    }
}

#[test]
fn level_reference_with_the_default_filter_can_pick_an_entity() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let uid = f
        .app
        .project
        .as_mut()
        .unwrap()
        .add_field_definition(None, "LevelTarget", "EntityRef", false)
        .unwrap();
    f.app.panel = Some(Panel::Level);
    let draw = |f: &mut Fixture, events| {
        let mut pick = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let project = f.app.project.as_ref().unwrap();
                    let field = project.level(f.source).unwrap()["fieldInstances"][0].clone();
                    let def = project.json["defs"]["levelFields"][0].clone();
                    pick = f.app.field_pick_controls(ui, f.source, None, &field, &def);
                });
            },
        );
        (pick.unwrap(), output)
    };
    let (rect, _) = draw(&mut f, vec![]);
    for pressed in [true, false] {
        draw(
            &mut f,
            vec![
                egui::Event::PointerMoved(rect.center()),
                button(rect.center(), pressed, egui::PointerButton::Primary),
            ],
        );
    }
    assert!(f.app.picking_entity_ref());
    f.app.navigate_level(f.target, false);
    frame(&mut f, &ctx, vec![], false);
    let (_, labels) = frame(&mut f, &ctx, vec![], false);
    assert!(
        labels.iter().any(|(text, _)| text == "Pick any entity in any level"),
        "{labels:?}"
    );
    click_entity(&mut f, &ctx, [96., 96.]);

    assert!(!f.app.picking_entity_ref());
    assert_eq!(f.app.selected, Some(f.source));
    let project = f.app.project.as_ref().unwrap();
    let field = &project.level(f.source).unwrap()["fieldInstances"][0];
    assert_eq!(field["defUid"], uid);
    assert_eq!(field["__value"]["entityIid"], f.target_iid);
}
fn click_entity(f: &mut Fixture, ctx: &egui::Context, local: [f32; 2]) {
    let (rect, _) = frame(f, ctx, vec![], false);
    let pos = f
        .app
        .viewport
        .to_screen(local, rect.min.into(), rect.size().into())
        .into();
    click(f, ctx, pos, false);
}
fn value(f: &Fixture) -> Value {
    f.app.project.as_ref().unwrap().layer(f.source,0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"].clone()
}

#[test]
fn canvas_reference_pick_crosses_levels_returns_to_source_and_follow_button_navigates() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    start(&mut f, &ctx);
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f)["entityIid"], f.target_iid);
    assert_eq!(f.app.selected, Some(f.source));
    assert!(f.app.panel.is_none(), "Cross-level return does not re-open the inspector");
    assert!(f.app.entity_index.is_none());
    assert!(!f.app.picking_entity_ref());
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let pos = labels.iter().find(|(s, _)| s == "Actor").unwrap().1;
    click(&mut f, &ctx, pos, true);
    assert_eq!(f.app.selected, Some(f.target));
    assert_eq!(f.app.entity_index, Some(0));
}

#[test]
fn cross_level_reference_pick_only_records_the_array_insertion_for_undo() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    start(&mut f, &ctx);
    assert_eq!(value(&f), json!([null]));
    f.app.dirty = false;
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(f.app.selected, Some(f.source));
    assert_eq!(value(&f)[0]["entityIid"], f.target_iid);
    assert!(f.app.dirty, "the cross-level link must be saved");

    f.app.undo_action(false);
    assert_eq!(value(&f), json!([]));
    f.app.undo_action(true);
    assert_eq!(value(&f), json!([null]));
}

#[test]
fn world_reference_picker_selects_the_level_before_picking_and_returns_in_world_view() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    f.app.project.as_mut().unwrap().level_mut(f.target).unwrap()["worldX"] = json!(640);
    start(&mut f, &ctx);
    f.app.show_panel(Some(Panel::World));
    f.app.camera_motion.cancel();
    f.app.viewport.center = [448., 128.];
    f.app.viewport.zoom = 0.35;
    frame(&mut f, &ctx, vec![], false);
    assert_eq!(
        ctx.data(|data| data.get_temp::<usize>(egui::Id::new("target-circles"))),
        Some(0),
        "World previews do not show reference targets"
    );
    click_entity(&mut f, &ctx, [736., 96.]);
    assert_eq!(f.app.selected, Some(f.target));
    assert!(f.app.picking_entity_ref());
    assert!(value(&f).is_null());
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let pos = rect.center();
    for pressed in [true, false] {
        frame(
            &mut f,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                button(pos, pressed, egui::PointerButton::Secondary),
            ],
            false,
        );
    }
    assert!(f.app.picking_entity_ref());
    assert_eq!(f.app.selected, Some(f.target));
    assert!(value(&f).is_null());

    // A subsequent click can pick an entity in the now-current level.
    click_entity(&mut f, &ctx, [736., 96.]);
    assert_eq!(value(&f)["entityIid"], f.target_iid);
    assert_eq!(f.app.selected, Some(f.source));
    assert!(f.app.world);
    assert_eq!(f.app.panel, Some(Panel::World));
    assert!(f.app.camera_motion.is_active());
    assert!(!f.app.picking_entity_ref());
}

#[test]
fn world_reference_picker_reports_an_empty_destination_after_selecting_it() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    project.level_mut(f.target).unwrap()["worldX"] = json!(640);
    project.layer_mut(f.target, 0).unwrap()["entityInstances"] = json!([]);
    start(&mut f, &ctx);
    f.app.show_panel(Some(Panel::World));
    f.app.camera_motion.cancel();
    f.app.viewport.center = [448., 128.];
    f.app.viewport.zoom = 0.35;
    frame(&mut f, &ctx, vec![], false);
    let (_, labels) = frame(&mut f, &ctx, vec![], false);
    assert!(labels.iter().any(|(text, _)| text == "Pick another Actor in any level"));
    click_entity(&mut f, &ctx, [736., 96.]);
    let (_, labels) = frame(&mut f, &ctx, vec![], false);
    assert!(labels.iter().any(|(text, _)| text == "No valid target in this level."));
    assert!(value(&f).is_null());
}
#[test]
fn clearing_or_removing_a_reference_cancels_the_picker_and_tidies_the_reciprocal() {
    for remove_slot in [false, true] {
        let mut f = fixture(true);
        let ctx = egui::Context::default();
        let project = f.app.project.as_mut().unwrap();
        let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
        project
            .update_field_definition(
                f.uid,
                &json!({"canBeNull":true,"symmetricalRef":true,"arrayMaxLength":2}),
            )
            .unwrap();
        project.add_entity(f.target, 0, actor, 128, 128).unwrap();
        let last = project.layer(f.target, 0).unwrap()["entityInstances"][1]["iid"].clone();
        project
            .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!([f.target_iid, last]))
            .unwrap();
        let before = value(&f);
        let reciprocal = |f: &Fixture| {
            f.app.project.as_ref().unwrap().layer(f.target, 0).unwrap()["entityInstances"][1]
                ["fieldInstances"][0]["__value"]
                .clone()
        };
        assert_eq!(reciprocal(&f)[0]["entityIid"], f.owner);
        start(&mut f, &ctx);
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        let pick = labels.iter().find(|(label, _)| label == "Canvas pick").unwrap().1;
        let (offset, expected) = if remove_slot {
            (46., json!([before[0]]))
        } else {
            (23., json!([before[0], null]))
        };
        click(&mut f, &ctx, pick + egui::vec2(offset, 0.), true);
        assert_eq!(value(&f), expected);
        assert!(!f.app.picking_entity_ref(), "remove_slot={remove_slot}");
        assert_eq!(reciprocal(&f), json!([]));

        f.app.undo_action(false);
        assert_eq!(value(&f), before);
        assert_eq!(reciprocal(&f)[0]["entityIid"], f.owner);
    }
}

#[test]
fn adding_or_removing_an_ordinary_array_item_cancels_an_active_reference_picker() {
    for (remove, expected) in [(true, json!([])), (false, json!([true, false]))] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let project = f.app.project.as_mut().unwrap();
        let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
        let flags = project
            .add_field_definition(Some(actor), "Flags", "Bool", true)
            .unwrap();
        project
            .set_field_editor_value(f.source, Some(&f.owner), flags, json!([true]))
            .unwrap();
        start(&mut f, &ctx);
        instance_fields_frame(&mut f, &ctx, vec![]);
        let output = instance_fields_frame(&mut f, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(image)
                    if remove && image.brush.is_some() && image.rect.size() == Vec2::splat(20.) =>
                {
                    Some(image.rect.center())
                }
                egui::Shape::Rect(button)
                    if !remove && button.fill == Color32::from_rgb(49, 55, 66) =>
                {
                    Some(button.rect.center())
                }
                _ => None,
            })
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .expect("array action icon");
        click_instance_fields(&mut f, &ctx, pos);
        let values = |f: &Fixture| fixture_entity(f)["fieldInstances"][1]["__value"].clone();
        assert_eq!(values(&f), expected);
        assert!(!f.app.picking_entity_ref());
        assert!(value(&f).is_null());
        f.app.undo_action(false);
        assert_eq!(values(&f), json!([true]));
    }
}

fn fixture_entity(f: &Fixture) -> &Value {
    &f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()["entityInstances"][0]
}

fn instance_fields_frame(
    f: &mut Fixture,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let entity = fixture_entity(f).clone();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400., 400.),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                f.app.instance_fields(ui, f.source, Some(&f.owner), &entity);
            });
        },
    )
}

fn click_instance_fields(f: &mut Fixture, ctx: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        instance_fields_frame(
            f,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                button(pos, pressed, egui::PointerButton::Primary),
            ],
        );
    }
}

fn point_and_reference_fixture() -> Fixture {
    let mut f = fixture(false);
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let point = project
        .add_field_definition(Some(actor), "Marker", "Point", false)
        .unwrap();
    project
        .set_field_editor_value(f.source, Some(&f.owner), point, json!({"cx":10,"cy":8}))
        .unwrap();
    f
}

fn field_label(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            Some(text.pos + text.galley.size() * 0.5)
        }
        _ => None,
    })
}

fn start_marker_pick(f: &mut Fixture, ctx: &egui::Context) {
    theme::configure(ctx);
    instance_fields_frame(f, ctx, vec![]);
    let output = instance_fields_frame(f, ctx, vec![]);
    let pos = field_label(&output, "10,8").expect("point picker button");
    click_instance_fields(f, ctx, pos);
    assert!(f.app.picking_point());
}

#[test]
fn changing_a_boolean_stops_point_picking_and_keeps_reference_picking() {
    for point in [true, false] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let project = f.app.project.as_mut().unwrap();
        let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
        if point {
            project.remove_field_definition(f.uid).unwrap();
            f.uid = project
                .add_field_definition(Some(actor), "Target", "Point", false)
                .unwrap();
            project
                .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!({"cx":10,"cy":8}))
                .unwrap();
        }
        let flag = project
            .add_field_definition(Some(actor), "Flag", "Bool", false)
            .unwrap();
        project
            .set_field_editor_value(f.source, Some(&f.owner), flag, json!(true))
            .unwrap();
        let picked_before = value(&f);
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        let pick = labels.iter().find(|(label, _)| label == "Canvas pick").unwrap().1;
        click(&mut f, &ctx, pick, true);
        assert_eq!(f.app.picking_point(), point);
        assert_eq!(f.app.picking_entity_ref(), !point);

        instance_fields_frame(&mut f, &ctx, vec![]);
        let output = instance_fields_frame(&mut f, &ctx, vec![]);
        let checkbox = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(0, 117, 255) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .expect("checked flag");
        click_instance_fields(&mut f, &ctx, checkbox);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], false);
        assert!(!f.app.picking_point());
        assert_eq!(f.app.picking_entity_ref(), !point);
        assert_eq!(value(&f), picked_before);
        f.app.undo_action(false);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], true);
        assert_eq!(value(&f), picked_before);
    }
}

#[test]
fn clearing_a_single_point_keeps_an_active_reference_picker() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let point = project
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    let original = json!({"cx":10,"cy":8});
    project
        .set_field_editor_value(f.source, Some(&f.owner), point, original.clone())
        .unwrap();
    start(&mut f, &ctx);
    instance_fields_frame(&mut f, &ctx, vec![]);
    let output = instance_fields_frame(&mut f, &ctx, vec![]);
    let clear = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "X" => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .expect("clear point");
    click_instance_fields(&mut f, &ctx, clear);
    assert!(fixture_entity(&f)["fieldInstances"][1]["__value"].is_null());
    assert!(f.app.picking_entity_ref());
    f.app.undo_action(false);
    assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], original);
}

#[test]
fn reference_picker_takes_canvas_input_until_cancelled_then_point_picking_resumes() {
    for reference_first in [false, true] {
        let mut f = point_and_reference_fixture();
        let ctx = egui::Context::default();
        let original = json!({"cx":10,"cy":8});
        if reference_first {
            start(&mut f, &ctx);
            start_marker_pick(&mut f, &ctx);
        } else {
            start_marker_pick(&mut f, &ctx);
            start(&mut f, &ctx);
        }
        assert!(f.app.picking_point() && f.app.picking_entity_ref());
        click_entity(&mut f, &ctx, [120., 104.]);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], original);
        assert!(f.app.picking_point() && f.app.picking_entity_ref());

        frame(
            &mut f,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            false,
        );
        assert!(!f.app.picking_entity_ref());
        assert!(f.app.picking_point(), "Escape only dismisses the reference picker");
        click_entity(&mut f, &ctx, [120., 104.]);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], json!({"cx":7,"cy":6}));
        assert!(value(&f).is_null());
        f.app.undo_action(false);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], original);

        f.app.entity_index = Some(0);
        start_marker_pick(&mut f, &ctx);
        start(&mut f, &ctx);
        f.app.navigate_level(f.target, false);
        assert!(!f.app.picking_point());
        assert!(f.app.picking_entity_ref());
    }
}

#[test]
fn completing_a_reference_shows_the_point_coordinates_while_picking_stays_active() {
    let mut f = point_and_reference_fixture();
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.add_entity(f.source, 0, actor, 96, 96).unwrap();
    let target = project.layer(f.source, 0).unwrap()["entityInstances"][1]["iid"].clone();
    start_marker_pick(&mut f, &ctx);
    start(&mut f, &ctx);
    let before = instance_fields_frame(&mut f, &ctx, vec![]);
    assert!(field_label(&before, "CANCEL").is_some());

    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f)["entityIid"], target);
    assert!(!f.app.picking_entity_ref());
    assert!(f.app.picking_point());
    instance_fields_frame(&mut f, &ctx, vec![]);
    let after = instance_fields_frame(&mut f, &ctx, vec![]);
    assert!(field_label(&after, "CANCEL").is_none());
    let coordinates = field_label(&after, "10,8").expect("restored coordinate label");
    click_instance_fields(&mut f, &ctx, coordinates);
    assert!(!f.app.picking_point());
    assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], json!({"cx":10,"cy":8}));
}

#[test]
fn restoring_level_history_cancels_point_picking_but_keeps_the_reference_picker() {
    let mut f = point_and_reference_fixture();
    let ctx = egui::Context::default();
    let original = json!({"cx":10,"cy":8});
    let changed = json!({"cx":7,"cy":6});
    start_marker_pick(&mut f, &ctx);
    click_entity(&mut f, &ctx, [120., 104.]);
    assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], changed);
    f.app.undo_action(false);

    for (redo, expected) in [(true, changed), (false, original)] {
        f.app.entity_index = Some(0);
        let label = if redo { "10,8" } else { "7,6" };
        let output = instance_fields_frame(&mut f, &ctx, vec![]);
        let point = field_label(&output, label).expect("point picker button");
        click_instance_fields(&mut f, &ctx, point);
        assert!(f.app.picking_point());
        start(&mut f, &ctx);
        f.app.undo_action(redo);
        assert_eq!(fixture_entity(&f)["fieldInstances"][1]["__value"], expected);
        assert!(!f.app.picking_point());
        assert!(f.app.picking_entity_ref());
        f.app.cancel_reference_pick();
    }
}

#[test]
fn entity_inspector_link_array_paints_the_target_icon_and_location() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let atlas = project.ensure_embedded_icons().unwrap();
    let tile = |x| json!({"tilesetUid":atlas,"x":x,"y":0,"w":16,"h":16});
    project
        .update_entity_definition(actor, &json!({"uiTileRect":tile(16)}))
        .unwrap();
    let icon = project
        .add_field_definition(Some(actor), "Icon", "Tile", false)
        .unwrap();
    project
        .update_field_definition(icon, &json!({"editorDisplayMode":"EntityTile","tilesetUid":atlas}))
        .unwrap();
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!([f.target_iid]))
        .unwrap();
    let texture = f.app.assets.embedded(&ctx).unwrap();
    let owner = f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()
        ["entityInstances"][0]
        .clone();
    let draw = |f: &mut Fixture| {
        ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(360.);
                f.app.instance_fields(ui, f.source, Some(&f.owner), &owner);
            });
        })
    };
    let atlas_width = texture.size()[0] as i64;
    for (tile, expected_x) in [
        (tile(32), 32.),
        (tile(atlas_width - 16), (atlas_width - 16) as f32),
        (tile(atlas_width - 8), 16.),
        (Value::Null, 16.),
    ] {
        f.app
            .project
            .as_mut()
            .unwrap()
            .set_field_editor_value(f.target, Some(&f.target_iid), icon, tile.clone())
            .unwrap();
        let output = draw(&mut f);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Other")
        }), "the row must show the referenced level");
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == texture.id()
                    && mesh.calc_bounds().width() == 18.
                    && mesh.vertices.iter().any(|v| v.uv.x == expected_x / texture.size_vec2().x))
        }), "the reference preview must use the correct 18-point atlas rectangle");
        assert_eq!(
            f.app.project.as_ref().unwrap().layer(f.target, 0).unwrap()
                ["entityInstances"][0]["fieldInstances"][1]["__value"],
            tile
        );
    }

    f.app
        .project
        .as_mut()
        .unwrap()
        .update_entity_definition(
            actor,
            &json!({
                "renderMode":"Tile", "uiTileRect":null,
                "tileRect":{"tilesetUid":atlas,"x":0,"y":0,"w":32,"h":16},
                "lineOpacity":0.75, "fillOpacity":0.4, "tileOpacity":0.55
            }),
        )
        .unwrap();
    let output = draw(&mut f);
    assert!(
        output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == texture.id()
                    && mesh.calc_bounds().size() == Vec2::splat(16.)
                    && mesh.vertices.iter().all(|v| v.color == Color32::WHITE.gamma_multiply(0.55)))
        }),
        "a non-square definition tile must fill the 16-point interior with its saved opacity"
    );
}

#[test]
fn escape_from_another_level_returns_to_source_without_changing_reference() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    start(&mut f, &ctx);
    f.app.navigate_level(f.target, false);
    frame(
        &mut f,
        &ctx,
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        false,
    );
    assert_eq!(f.app.selected, Some(f.source));
    assert!(value(&f).is_null());
}
#[test]
fn canvas_picker_rejects_self_and_duplicate_array_references() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    start(&mut f, &ctx);
    click_entity(&mut f, &ctx, [32., 32.]);
    assert_eq!(value(&f), json!([null]), "The source cannot be its own target");
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f).as_array().unwrap().len(), 1);
    start(&mut f, &ctx);
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f)[0]["entityIid"], f.target_iid);
    assert_eq!(value(&f).as_array().unwrap().len(), 2);
    assert!(value(&f)[1].is_null(), "Already-referenced targets are excluded");
    f.app.cancel_field_pick();
    assert!(!f.app.picking_field());
    assert_eq!(f.app.selected, Some(f.source));
    assert_eq!(value(&f).as_array().unwrap().len(), 2, "Cancel retains the new slot");
}
#[test]
fn canvas_picker_skips_invalid_entity_covering_an_allowed_target() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let p = f.app.project.as_mut().unwrap();
    let wrong = p.add_entity_def("OtherType", 16, 16).unwrap();
    p.add_entity(f.target, 0, wrong, 96, 96).unwrap();
    start(&mut f, &ctx);
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(
        value(&f)["entityIid"],
        f.target_iid,
        "Picking must filter allowed targets before hit testing"
    );
}

#[test]
fn canvas_reference_pick_crosses_worlds_and_cancel_button_restores_source() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let p = f.app.project.as_mut().unwrap();
    let world = p.add_world("Elsewhere").unwrap();
    f.source.world = Some(0);
    f.target.world = Some(0);
    f.target = p.move_level_to_world(f.target, Some(world), 0).unwrap();
    f.app.navigate_level(f.source, false);
    f.app.panel = Some(Panel::Entity);
    start(&mut f, &ctx);
    f.app.navigate_world(Some(world));
    f.app.navigate_level(f.target, false);
    f.app.world = false;
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f)["entityIid"], f.target_iid);
    assert_eq!(f.app.selected, Some(f.source));
    assert_eq!(f.app.active_world, Some(0));
    assert!(f.app.world);
    f.app.show_panel(None);
    f.app.panel = Some(Panel::Entity);
    f.app.entity_index = Some(0);
    start(&mut f, &ctx);
    f.app.navigate_world(Some(world));
    f.app.world = false;
    f.app.fit = false;
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let pos = rect.center();
    frame(
        &mut f,
        &ctx,
        vec![
            egui::Event::PointerMoved(pos),
            button(pos, true, egui::PointerButton::Secondary),
        ],
        false,
    );
    frame(
        &mut f,
        &ctx,
        vec![button(pos, false, egui::PointerButton::Secondary)],
        false,
    );
    assert_eq!(f.app.selected, Some(f.target));
    assert!(f.app.picking_entity_ref(), "Right click leaves the picker open");
    let (_, labels) = frame(&mut f, &ctx, vec![], false);
    let cancel = labels.iter().find(|(s, _)| s == "CANCEL").unwrap().1;
    click(&mut f, &ctx, cancel, false);
    assert_eq!(f.app.selected, Some(f.source));
    assert!(f.app.world);
    assert!(!f.app.picking_entity_ref());
}

#[test]
fn same_level_reference_pick_keeps_the_inspector_and_camera() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.add_entity(f.source, 0, actor, 96, 96).unwrap();
    let target = project.layer(f.source, 0).unwrap()["entityInstances"][1]["iid"].clone();
    let center = f.app.viewport.center;
    start(&mut f, &ctx);
    assert_eq!(f.app.panel, Some(Panel::Entity));
    frame(&mut f, &ctx, vec![], false);
    let (rect, labels) = frame(&mut f, &ctx, vec![], false);
    assert!(labels.iter().any(|(text, pos)| {
        text == "Pick another Actor in any level" && pos.y > rect.bottom() * 0.85
    }), "{labels:?}");
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f)["entityIid"], target);
    assert_eq!(f.app.viewport.center, center);
    assert_eq!(f.app.panel, Some(Panel::Entity));
    assert_eq!(f.app.entity_index, Some(0));
    assert!(!f.app.picking_entity_ref());
    f.app.undo_action(false);
    assert!(value(&f).is_null());
    f.app.undo_action(true);
    assert_eq!(value(&f)["entityIid"], target);
}

#[test]
fn reference_target_hints_and_clicks_match_layer_visibility_and_field_restrictions() {
    for restriction in ["OnlyTags", "OnlySpecificEntity", "same-level", "hidden"] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        let patch = match restriction {
            "OnlyTags" => json!({"allowedRefs":"OnlyTags","allowedRefTags":["Enemy"]}),
            "OnlySpecificEntity" => {
                json!({"allowedRefs":"OnlySpecificEntity","allowedRefsEntityUid":999})
            }
            "same-level" => json!({"allowOutOfLevelRef":false}),
            _ => json!({}),
        };
        f.app
            .project
            .as_mut()
            .unwrap()
            .update_field_definition(f.uid, &patch)
            .unwrap();
        if restriction == "hidden" {
            f.app
                .project
                .as_mut()
                .unwrap()
                .layer_mut(f.target, 0)
                .unwrap()["visible"] = false.into();
        }
        start(&mut f, &ctx);
        f.app.navigate_level(f.target, false);
        frame(&mut f, &ctx, vec![], false);
        let (_, labels) = frame(&mut f, &ctx, vec![], false);
        let has_error = labels
            .iter()
            .any(|(s, _)| s == "No valid target in this level."
                || s == "You can only pick a reference in the same level for this value.");
        assert_eq!(
            has_error,
            restriction != "hidden",
            "{restriction}: {labels:?}"
        );
        assert_eq!(
            ctx.data(|d| d.get_temp::<usize>(egui::Id::new("target-circles"))),
            Some(if restriction == "hidden" { 2 } else { 0 })
        );
        click_entity(&mut f, &ctx, [96., 96.]);
        assert!(value(&f).is_null(), "{restriction}");
        f.app.cancel_field_pick();
        let actor = f.app.project.as_ref().unwrap().json["defs"]["entities"][0]["uid"]
            .as_i64()
            .unwrap();
        let p = f.app.project.as_mut().unwrap();
        match restriction {
            "OnlyTags" => {
                p.update_entity_definition(actor, &json!({"tags":["Enemy"]}))
                    .unwrap();
            }
            "OnlySpecificEntity" => {
                p.update_field_definition(f.uid, &json!({"allowedRefsEntityUid":actor}))
                    .unwrap();
            }
            "same-level" => {
                p.update_field_definition(f.uid, &json!({"allowOutOfLevelRef":true}))
                    .unwrap();
            }
            _ => {
                p.layer_mut(f.target, 0).unwrap()["visible"] = true.into();
            }
        }
        start(&mut f, &ctx);
        f.app.navigate_level(f.target, false);
        frame(&mut f, &ctx, vec![], false);
        assert_eq!(
            ctx.data(|d| d.get_temp::<usize>(egui::Id::new("target-circles"))),
            Some(2),
            "{restriction}: valid target has two circles"
        );
        click_entity(&mut f, &ctx, [96., 96.]);
        assert_eq!(value(&f)["entityIid"], f.target_iid, "{restriction}");
    }
}

#[test]
fn point_path_cursor_draws_the_source_line_from_the_last_accepted_point() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Path", "Point", true)
        .unwrap();
    project
        .update_field_definition(f.uid, &json!({"editorDisplayMode":"PointPath"}))
        .unwrap();
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!([{"cx":4,"cy":4}]))
        .unwrap();
    let field = &mut project.layer_mut(f.source, 0).unwrap()
        ["entityInstances"][0]["fieldInstances"][0];
    field["realEditorValues"][0]["extension"] = json!({"preserve":true});
    let first_wrapper = field["realEditorValues"][0].clone();
    let color = canvas::entity_smart_color(
        project,
        &project.layer(f.source, 0).unwrap()["entityInstances"][0],
        &project.json["defs"]["entities"][0],
        true,
    );
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    click(
        &mut f,
        &ctx,
        labels
            .iter()
            .find(|(label, _)| label == "Canvas pick")
            .unwrap()
            .1,
        true,
    );
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let view = f.app.viewport.clone();
    let screen = |pos| egui::Pos2::from(view.to_screen(pos, rect.min.into(), rect.size().into()));
    let draw = |f: &mut Fixture, target| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 400.),
                )),
                events: vec![egui::Event::PointerMoved(target)],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                    f.app.canvas_input(ctx, rect, &response);
                    f.app.draw_field_pick_targets(ui, rect);
                });
            },
        )
    };
    let line_at = |out: &egui::FullOutput, start, end| {
        out.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::LineSegment {points,stroke}
            if *points == [start,end] && *stroke == Stroke::new(1_f32,color))
        })
    };
    let first = draw(&mut f, screen([130., 146.]));
    assert!(line_at(&first, screen([72., 72.]), screen([136., 152.])));
    assert_eq!(first.platform_output.cursor_icon, egui::CursorIcon::Default);
    click(&mut f, &ctx, screen([130., 146.]), false);
    let field = &f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()
        ["entityInstances"][0]["fieldInstances"][0];
    assert_eq!(field["__value"], json!([{"cx":4,"cy":4},{"cx":8,"cy":9}]));
    assert_eq!(field["realEditorValues"][0], first_wrapper);
    assert!(f.app.point_pick.as_ref().unwrap().point_target.is_none());
    frame(&mut f, &ctx, vec![], false);
    assert!(f.app.point_pick.as_ref().unwrap().point_target.is_none());
    let next = draw(&mut f, screen([178., 146.]));
    assert!(line_at(&next, screen([136., 152.]), screen([184., 152.])));
    assert!(!line_at(
        &draw(&mut f, screen([32., 32.])),
        screen([136., 152.]),
        screen([40., 40.])
    ));
}

#[test]
fn point_picker_stays_armed_while_entity_handles_resize_with_either_button() {
    for button_kind in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        let project = f.app.project.as_mut().unwrap();
        let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
        project.remove_field_definition(f.uid).unwrap();
        f.uid = project
            .add_field_definition(Some(actor), "Target", "Point", false)
            .unwrap();
        project
            .update_entity_definition(actor, &json!({"resizableX":true,"pivotX":0,"pivotY":0}))
            .unwrap();
        f.app.grid = false;
        f.app.repaint = Some(ctx.clone());
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        click(
            &mut f,
            &ctx,
            labels
                .iter()
                .find(|(label, _)| label == "Canvas pick")
                .unwrap()
                .1,
            true,
        );
        let (rect, _) = frame(&mut f, &ctx, vec![], false);
        let view = f.app.viewport.clone();
        let screen =
            |pos| egui::Pos2::from(view.to_screen(pos, rect.min.into(), rect.size().into()));

        let handle = screen([52., 40.]);
        let finish = screen([84., 40.]);
        frame(
            &mut f,
            &ctx,
            vec![
                egui::Event::PointerMoved(handle),
                button(handle, true, button_kind),
            ],
            false,
        );
        assert!(matches!(
            f.app.gesture,
            Some(tools::Gesture::EntityResize { .. })
        ));
        assert!(f.app.picking_point());
        frame(&mut f, &ctx, vec![egui::Event::PointerMoved(finish)], false);
        assert_eq!(
            f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()["entityInstances"][0]["width"],
            47 // Level X 84 minus the left edge 32 and handle radius 5.
        );
        frame(
            &mut f,
            &ctx,
            vec![button(finish, false, button_kind)],
            false,
        );
        assert!(f.app.gesture.is_none());
        assert!(f.app.picking_point());
        assert!(value(&f).is_null());

        let primary = egui::PointerButton::Primary;
        let empty = screen([104., 40.]);
        frame(
            &mut f,
            &ctx,
            vec![
                egui::Event::PointerMoved(empty),
                button(empty, true, primary),
            ],
            false,
        );
        frame(&mut f, &ctx, vec![egui::Event::PointerMoved(finish)], false);
        assert!(f.app.gesture.is_none());
        assert!(
            f.app.async_paint.is_none(),
            "hovering the resize handle does not renew the Point timer"
        );
        frame(&mut f, &ctx, vec![button(finish, false, primary)], false);
        assert_eq!(value(&f), json!({"cx":5,"cy":2}));
        assert!(!f.app.picking_point());
    }
}

#[test]
fn point_picker_does_not_consume_a_primary_press_on_the_level_ruler() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    click(
        &mut f,
        &ctx,
        labels
            .iter()
            .find(|(label, _)| label == "Canvas pick")
            .unwrap()
            .1,
        true,
    );
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let view = f.app.viewport.clone();
    let screen = |pos| egui::Pos2::from(view.to_screen(pos, rect.min.into(), rect.size().into()));
    let handle = screen([260., 128.]);
    let finish = screen([292., 128.]);
    let primary = egui::PointerButton::Primary;
    frame(
        &mut f,
        &ctx,
        vec![
            egui::Event::PointerMoved(handle),
            button(handle, true, primary),
            egui::Event::PointerMoved(finish),
        ],
        false,
    );
    assert!(f.app.level_resize.is_some());
    assert!(f.app.picking_point());
    frame(
        &mut f,
        &ctx,
        vec![
            egui::Event::PointerMoved(finish),
            button(finish, false, primary),
        ],
        false,
    );
    assert_eq!(
        f.app.project.as_ref().unwrap().level(f.source).unwrap()["pxWid"],
        288
    );
    assert!(f.app.picking_point());
    assert!(value(&f).is_null());
}

#[test]
fn point_picker_routes_primary_press_to_selection_only_when_started_over_an_entity() {
    for starts_on_entity in [true, false] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        let project = f.app.project.as_mut().unwrap();
        let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
        project.remove_field_definition(f.uid).unwrap();
        f.uid = project
            .add_field_definition(Some(actor), "Target", "Point", false)
            .unwrap();
        project.add_entity(f.source, 0, actor, 96, 96).unwrap();
        let initial = json!({"cx":4,"cy":4});
        project
            .set_field_editor_value(f.source, Some(&f.owner), f.uid, initial.clone())
            .unwrap();
        f.app.grid = false;
        f.app.repaint = Some(ctx.clone());
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        let control = labels
            .iter()
            .find(|(label, _)| label == "Canvas pick")
            .unwrap()
            .1;
        click(&mut f, &ctx, control, true);
        let (rect, _) = frame(&mut f, &ctx, vec![], false);
        let screen = |local| {
            egui::Pos2::from(
                Viewport {
                    center: [128.; 2],
                    zoom: 1.,
                }
                .to_screen(local, rect.min.into(), rect.size().into()),
            )
        };
        let body = screen([96., 96.]);
        let empty = screen([128., 96.]);
        let (start, end) = if starts_on_entity {
            (body, empty)
        } else {
            (empty, body)
        };
        let primary = egui::PointerButton::Primary;
        frame(
            &mut f,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true, primary),
                egui::Event::PointerMoved(end),
            ],
            false,
        );
        assert_eq!(f.app.picking_point(), !starts_on_entity);
        assert_eq!(
            value(&f),
            initial,
            "the Point field stays live but unmodified before release"
        );
        if starts_on_entity {
            assert_eq!(f.app.entity_index, Some(1));
            assert!(matches!(
                f.app.gesture,
                Some(tools::Gesture::SelectionMove { .. })
            ));
        } else {
            assert!(
                f.app.async_paint.is_none(),
                "hovering an entity does not renew the Point timer"
            );
        }
        frame(&mut f, &ctx, vec![button(end, false, primary)], false);
        assert!(!f.app.picking_point());
        let moved_entity =
            &f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()["entityInstances"][1];
        if starts_on_entity {
            assert_eq!(moved_entity["px"], json!([128, 96]));
            assert_eq!(value(&f), initial);
        } else {
            assert_eq!(moved_entity["px"], json!([96, 96]));
            assert_eq!(value(&f), json!({"cx":6,"cy":6}));
        }
        f.app.undo_action(false);
        assert_eq!(value(&f), initial);
        assert_eq!(
            f.app.project.as_ref().unwrap().layer(f.source, 0).unwrap()["entityInstances"][1]["px"],
            json!([96, 96])
        );
        assert!(!f.app.history.can_undo());
    }
}

#[test]
fn point_picker_preview_and_release_use_the_rounded_cell_at_a_boundary() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    f.app.viewport.zoom = 2.;
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    click(
        &mut f,
        &ctx,
        labels.iter().find(|(s, _)| s == "Canvas pick").unwrap().1,
        true,
    );
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let edge = egui::pos2(rect.center().x - 97., rect.center().y - 112.);
    frame(&mut f, &ctx, vec![egui::Event::PointerMoved(edge)], false);
    let target = f.app.point_pick.as_ref().unwrap().point_target;
    assert_eq!(target, Some([5, 4]));
    click(&mut f, &ctx, edge, false);
    assert_eq!(value(&f), json!({"cx":5,"cy":4}));
}

#[test]
fn point_picker_releases_at_final_cell_with_the_last_held_move_deadline() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    project
        .update_field_definition(f.uid, &json!({"editorDisplayMode":"Points"}))
        .unwrap();
    let layer_uid = project.layer(f.source, 0).unwrap()["layerDefUid"]
        .as_i64()
        .unwrap();
    project
        .update_layer_properties(layer_uid, &json!({"useAsyncRender":true}))
        .unwrap();
    let original = json!({"cx":4,"cy":4});
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, original.clone())
        .unwrap();
    f.app.repaint = Some(ctx.clone());
    let open_picker = |f: &mut Fixture| {
        let (_, labels) = frame(f, &ctx, vec![], true);
        click(
            f,
            &ctx,
            labels.iter().find(|(s, _)| s == "Canvas pick").unwrap().1,
            true,
        );
        assert!(f.app.picking_field());
    };
    open_picker(&mut f);
    let key = f.source;
    let draw = |f: &mut Fixture, time, events| {
        ctx.run(
            egui::RawInput {
                time: Some(time),
                events,
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 400.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                    f.app.canvas_input(ctx, rect, &response);
                    f.app.draw_project_canvas(ctx, ui.painter(), rect, key);
                });
            },
        )
    };
    let point_at = |output: &egui::FullOutput, center| {
        output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Path(p)
            if p.points.len()==4 && p.points[0]==center-egui::vec2(0.,4.) && p.points[2]==center+egui::vec2(0.,4.)))
    };
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let screen = |local| {
        egui::Pos2::from(
            Viewport {
                center: [128.; 2],
                zoom: 1.,
            }
            .to_screen(local, rect.min.into(), rect.size().into()),
        )
    };
    let old_point = screen([72., 72.]);
    let mut target = screen([104., 72.]);
    let primary = egui::PointerButton::Primary;
    let started = draw(
        &mut f,
        1.0,
        vec![
            egui::Event::PointerMoved(old_point),
            button(old_point, true, primary),
        ],
    );
    assert!(point_at(&started, old_point));
    assert!(
        f.app.async_paint.is_none(),
        "motion before a press does not start the timer"
    );
    draw(
        &mut f,
        1.05,
        vec![egui::Event::PointerMoved(screen([88., 72.]))],
    );
    draw(&mut f, 1.15, vec![egui::Event::PointerMoved(target)]);
    assert_eq!(value(&f), original);
    let pending = draw(&mut f, 1.29, vec![button(target, false, primary)]);
    assert_eq!(value(&f), json!({"cx":6,"cy":4}));
    assert_eq!(f.app.panel, Some(Panel::Entity));
    assert!(!f.app.picking_field());
    assert!(point_at(&pending, old_point));
    assert!(!point_at(&pending, target));
    assert!(point_at(&draw(&mut f, 1.399, vec![]), old_point));
    let settled = draw(&mut f, 1.401, vec![]);
    assert!(point_at(&settled, target));
    assert!(!point_at(&settled, old_point));

    open_picker(&mut f);
    draw(&mut f, 1.99, vec![]);
    target = screen([120., 72.]);
    draw(
        &mut f,
        2.0,
        vec![
            egui::Event::PointerMoved(target),
            button(target, true, primary),
        ],
    );
    let clicked = draw(&mut f, 2.01, vec![button(target, false, primary)]);
    assert_eq!(value(&f), json!({"cx":7,"cy":4}));
    assert!(point_at(&clicked, target));
    assert!(f.app.async_paint.is_none());
}

#[test]
fn opening_a_panel_stops_point_picking_and_preserves_reference_picking() {
    for kind in ["Point", "EntityRef"] {
        let mut f = fixture(false);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        if kind == "Point" {
            let project = f.app.project.as_mut().unwrap();
            let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
            project.remove_field_definition(f.uid).unwrap();
            f.uid = project
                .add_field_definition(Some(actor), "Target", kind, false)
                .unwrap();
            project
                .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!({"cx":4,"cy":6}))
                .unwrap();
        }
        let original = value(&f);
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        let pick = labels.iter().find(|(label, _)| label == "Canvas pick").unwrap().1;
        click(&mut f, &ctx, pick, true);
        assert!(f.app.picking_field(), "{kind} must start picking");

        f.app.show_panel(Some(Panel::Layers));
        assert_eq!(f.app.panel, Some(Panel::Layers));
        assert!(!f.app.picking_point());
        assert_eq!(f.app.picking_entity_ref(), kind == "EntityRef");
        f.app.show_panel(None);
        assert_eq!(f.app.picking_entity_ref(), kind == "EntityRef");
        assert_eq!(value(&f), original);
        assert!(!f.app.history.can_undo());
    }
}

#[test]
fn single_point_picker_can_be_cancelled_in_the_entity_inspector() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    let original = json!({"cx":10,"cy":8});
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, original.clone())
        .unwrap();

    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let pick = labels.iter().find(|(label, _)| label == "Canvas pick").unwrap().1;
    click(&mut f, &ctx, pick, true);
    assert!(f.app.picking_point());
    assert_eq!(f.app.panel, Some(Panel::Entity), "the cancel control must remain accessible");

    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let cancel = labels.iter().find(|(label, _)| label == "CANCEL").unwrap().1;
    click(&mut f, &ctx, cancel, true);
    assert!(!f.app.picking_point());
    assert_eq!(value(&f), original);
    assert!(!f.app.history.can_undo());

    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let clear = labels.iter().find(|(label, _)| label == "X").unwrap().1;
    click(&mut f, &ctx, clear, true);
    assert_eq!(value(&f), Value::Null);
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    assert!(labels.iter().any(|(label, _)| label == "<NO POINT>"));
    assert!(!labels.iter().any(|(label, _)| label == "X"));
}

#[test]
fn point_array_pick_button_reserves_the_source_summary_and_row_heights() {
    let mut f = fixture(false);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Target", "Point", true)
        .unwrap();
    project
        .set_field_editor_value(
            f.source,
            Some(&f.owner),
            f.uid,
            json!([{"cx":4,"cy":6},{"cx":10,"cy":8}]),
        )
        .unwrap();
    for style in ["Points", "PointPathLoop"] {
        f.app.project.as_mut().unwrap()
            .update_field_definition(f.uid, &json!({"editorDisplayMode":style}))
            .unwrap();
        let (_, labels) = frame(&mut f, &ctx, vec![], true);
        let center_y = |label| labels.iter().find(|(s, _)| s == label).unwrap().1.y;
        let button_top = center_y("Canvas pick") - 11.5;
        if style == "Points" {
            let gap = button_top - center_y("<4,6>, <10,8>");
            assert!((30. ..=34.).contains(&gap), "compact summary: {gap}");
        } else {
            let row_height = center_y("10,8") - center_y("4,6");
            assert_eq!(row_height, 28., "point rows have no intervening gap");
            let gap = button_top - center_y("10,8");
            assert!((15. ..=17.).contains(&gap), "last point row: {gap}");
        }
    }
}

#[test]
fn point_array_marks_only_expanded_required_slots_in_the_inspector() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    theme::configure(&ctx);
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Waypoints", "Point", true)
        .unwrap();
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!([null, {"cx":10,"cy":8}]))
        .unwrap();

    for (mode, expected_text, required) in [
        ("PointPathLoop", "POINT REQUIRED!", true),
        ("Points", ", <10,8>", false),
    ] {
        let project = f.app.project.as_mut().unwrap();
        project
            .update_field_definition(f.uid, &json!({"canBeNull":false,"editorDisplayMode":mode}))
            .unwrap();
        let entity = project.layer(f.source, 0).unwrap()["entityInstances"][0].clone();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_width(330.);
                f.app.instance_fields(ui, f.source, Some(&f.owner), &entity);
            });
        });
        let has_label = |expected| output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == expected)
        });
        let error_headers = output.shapes.iter().filter(|shape| {
            matches!(&shape.shape, egui::Shape::Vec(parts) if parts.iter().any(|part|
                matches!(part, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(121, 17, 5))
            ))
        }).count();
        assert_eq!(error_headers, usize::from(required), "{mode}");
        assert!(has_label(expected_text), "{mode}");
        assert!(!has_label("Value required"), "{mode}");
    }
}

#[test]
fn point_array_coordinates_are_read_only_and_required_slots_can_be_filled() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    let project = f.app.project.as_mut().unwrap();
    let actor = project.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    project.remove_field_definition(f.uid).unwrap();
    f.uid = project
        .add_field_definition(Some(actor), "Waypoints", "Point", true)
        .unwrap();
    project
        .update_field_definition(
            f.uid,
            &json!({"canBeNull":false,"arrayMaxLength":2,"editorDisplayMode":"PointPathLoop"}),
        )
        .unwrap();
    project
        .set_field_editor_value(f.source, Some(&f.owner), f.uid, json!([null,{"cx":8,"cy":8}]))
        .unwrap();

    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let coordinates = labels.iter().find(|(label, _)| label == "8,8").unwrap().1;
    click(&mut f, &ctx, coordinates, true);
    assert!(!f.app.picking_field(), "existing coordinates are explanatory text");

    let required = labels.iter().find(|(label, _)| label == "POINT REQUIRED!").unwrap().1;
    click(&mut f, &ctx, required, true);
    assert_eq!(f.app.point_pick.as_ref().unwrap().index, Some(0));
    assert_eq!(f.app.panel, Some(Panel::Entity));
    let (_, controls) = frame(&mut f, &ctx, vec![], true);
    let cancel = controls.iter().find(|(label, _)| label == "CANCEL").unwrap().1;
    click(&mut f, &ctx, cancel, true);
    assert!(!f.app.picking_field());
    click(&mut f, &ctx, required, true);
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let target =
        f.app.viewport.to_screen([104., 120.], rect.min.into(), rect.size().into());
    click(&mut f, &ctx, target.into(), false);
    assert_eq!(value(&f), json!([{"cx":6,"cy":7},{"cx":8,"cy":8}]));
    assert!(!f.app.picking_field());
}

#[test]
fn point_array_canvas_pick_retains_offset_parallax_geometry_and_editor_length_diagnostics() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    let p = f.app.project.as_mut().unwrap();
    let actor = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    p.update_entity_definition(actor, &json!({"pivotX":0.5,"pivotY":1}))
        .unwrap();
    p.remove_field_definition(f.uid).unwrap();
    f.uid = p
        .add_field_definition(Some(actor), "Path", "Point", true)
        .unwrap();
    p.update_field_definition(f.uid, &json!({"arrayMinLength":3}))
        .unwrap();
    let layer_uid = p.layer(f.source, 0).unwrap()["layerDefUid"]
        .as_i64()
        .unwrap();
    p.update_layer_properties(layer_uid, &json!({"pxOffsetX":24,"pxOffsetY":-8,"parallaxFactorX":0.5,"parallaxFactorY":0.5,"parallaxScaling":true})).unwrap();
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let pos = labels.iter().find(|(s, _)| s == "Canvas pick").unwrap().1;
    click(&mut f, &ctx, pos, true);
    assert!(f.app.picking_field());
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    // Coords uses [64 + 24, 64 - 8] as the layer input origin.
    let world = [108., 92.];
    let pointer = f
        .app
        .viewport
        .to_screen(world, rect.min.into(), rect.size().into())
        .into();
    click(&mut f, &ctx, pointer, false);
    assert_eq!(value(&f), json!([{"cx":2,"cy":4}]));
    assert!(
        f.app.picking_field(),
        "Point arrays continue collecting a path"
    );
    assert_eq!(
        f.app
            .history
            .level_change_bounds(f.app.project.as_ref().unwrap(), f.source, false),
        Some([48., 8., 16., 16.]),
        "point picking marks the owner in raw layer coordinates"
    );
    assert!(
        !f.app
            .project
            .as_ref()
            .unwrap()
            .field_value_errors(f.source, Some(&f.owner), f.uid)
            .unwrap()
            .is_empty()
    );
    click(&mut f, &ctx, pointer, false);
    assert_eq!(
        value(&f),
        json!([{"cx":2,"cy":4}]),
        "duplicate points are rejected"
    );
    assert!(f.app.picking_field());

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("PointPath.ldtk");
    f.app.save_async_at(path.clone());
    f.app
        .saving
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let saved = Project::load(path).unwrap();
    assert_eq!(
        saved.layer(f.source, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"],
        value(&f)
    );
    assert!(
        f.app.picking_field(),
        "saving does not close the point picker"
    );

    frame(
        &mut f,
        &ctx,
        vec![button(pointer, true, egui::PointerButton::Secondary)],
        false,
    );
    assert!(
        !f.app.picking_field(),
        "right press cancels before release"
    );
    assert!(f.app.panel == Some(Panel::Entity));
    assert_eq!(
        value(&f),
        json!([{"cx":2,"cy":4}]),
        "cancel keeps points already accepted"
    );
}
