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
    let (_, labels) = frame(f, ctx, vec![], true);
    let pos = labels.iter().find(|(s, _)| s == "Canvas pick").unwrap().1;
    click(f, ctx, pos, true);
    assert!(f.app.picking_entity_ref());
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
    assert!(!f.app.picking_entity_ref());
    let (_, labels) = frame(&mut f, &ctx, vec![], true);
    let pos = labels.iter().find(|(s, _)| s == "Actor").unwrap().1;
    click(&mut f, &ctx, pos, true);
    assert_eq!(f.app.selected, Some(f.target));
    assert_eq!(f.app.entity_index, Some(0));
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
    assert_eq!(value(&f), json!([]), "The source cannot be its own target");
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(value(&f).as_array().unwrap().len(), 1);
    start(&mut f, &ctx);
    f.app.navigate_level(f.target, false);
    click_entity(&mut f, &ctx, [96., 96.]);
    assert_eq!(
        value(&f).as_array().unwrap().len(),
        1,
        "Already-referenced targets are excluded"
    );
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
fn canvas_reference_pick_crosses_worlds_and_right_click_restores_source() {
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
    assert_eq!(f.app.selected, Some(f.source));
    assert!(!f.app.picking_entity_ref());
}

#[test]
fn reference_target_hints_and_clicks_obey_visibility_tags_specific_type_and_level_limit() {
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
        let (_, labels) = frame(&mut f, &ctx, vec![], false);
        assert!(
            labels
                .iter()
                .any(|(s, _)| s == "No valid target in this level."
                    || s == "You can only pick a reference in the same level for this value."),
            "{restriction}: {labels:?}"
        );
        assert_eq!(
            ctx.data(|d| d.get_temp::<usize>(egui::Id::new("target-circles"))),
            Some(0)
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
fn point_array_canvas_pick_retains_offset_parallax_geometry_and_editor_length_diagnostics() {
    let mut f = fixture(true);
    let ctx = egui::Context::default();
    let p = f.app.project.as_mut().unwrap();
    let actor = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
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
    assert!(f.app.field_pick.is_some());
    let (rect, _) = frame(&mut f, &ctx, vec![], false);
    let world = canvas::layer_to_world(
        f.app.project.as_ref().unwrap(),
        f.source,
        0,
        &f.app.viewport,
        [40., 72.],
    );
    let pointer = f
        .app
        .viewport
        .to_screen(world, rect.min.into(), rect.size().into())
        .into();
    click(&mut f, &ctx, pointer, false);
    assert_eq!(value(&f), json!([{"cx":2,"cy":4}]));
    assert!(
        f.app.field_pick.is_some(),
        "Point arrays continue collecting a path"
    );
    assert_eq!(
        f.app
            .history
            .level_change_bounds(f.app.project.as_ref().unwrap(), f.source, false),
        Some([48., 8., 16., 16.]),
        "hidden inspector must still mark the Point owner in raw layer coordinates"
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
}
