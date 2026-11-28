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
    type_focused(app, ctx, value);
}
fn type_focused(app: &mut EditorApp, ctx: &egui::Context, value: &str) {
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
fn default_tileset_row_opens_the_library_and_reports_bound_tile_size() {
    let (mut app, ctx, uid) = editor("Tiles");
    app.panel = Some(Panel::Layers);
    let before = app.project.as_ref().unwrap().json.clone();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let create = label(&out, "CREATE NEW TILESET");
    assert!(create.left() > label(&out, "-- Tileset required --").right());
    click(&mut app, &ctx, create.center());
    assert_eq!(app.panel, Some(Panel::Tilesets));
    assert_eq!(app.project.as_ref().unwrap().json, before);
    app.dismiss_panel();
    assert!(app.panel.is_none());

    let p = app.project.as_mut().unwrap();
    let atlas = p.ensure_embedded_icons().unwrap();
    p.update_layer_properties(uid, &json!({"tilesetDefUid":atlas}))
        .unwrap();
    app.panel = Some(Panel::Layers);
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let picker = label(&out, "⚙ Internal icons");
    let size = label(&out, "16px tiles");
    assert!(size.left() > picker.right());
    assert!((size.center().y - picker.center().y).abs() <= 2.);
    assert!(labels(&out, "CREATE NEW TILESET").is_empty());
    for (grid, warns) in [(10, true), (8, false), (32, true), (16, false)] {
        app.project
            .as_mut()
            .unwrap()
            .change_layer_grid_size(uid, grid)
            .unwrap();
        let out = frame(&mut app, &ctx, vec![]);
        let message = format!(
            "Warning: the TILESET grid (16px) differs from the LAYER grid ({grid}px), and the values aren't multiples, which can lead to unexpected behaviors when adding a group of tiles."
        );
        assert_eq!(
            !labels(&out, &message).is_empty(),
            warns,
            "layer grid {grid}"
        );
    }
}

#[test]
fn tileset_picker_groups_multitag_atlases_without_selecting_headings() {
    let (mut app, ctx, uid) = editor("Tiles");
    let project = app.project.as_mut().unwrap();
    let mut canopy = None;
    for (name, tags) in [
        ("Plain", &[][..]),
        ("Canopy", &["forest", "City"][..]),
        ("Road", &["City"][..]),
    ] {
        let atlas = project
            .add_tileset(Path::new(&format!("{name}.png")), 16, 32, 32)
            .unwrap();
        project
            .update_tileset_definition(atlas, &json!({"tags":tags}))
            .unwrap();
        if name == "Canopy" {
            canopy = Some(atlas);
        }
    }
    frame(&mut app, &ctx, vec![]);
    let closed = frame(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        label(&closed, "-- Tileset required --").center(),
    );
    let opened = frame(&mut app, &ctx, vec![]);
    let [untagged, city, forest, integrated] =
        ["Untagged", "City", "forest", "Integrated LDtk tilesets"].map(|name| label(&opened, name));
    let canopy_rows = labels(&opened, "Canopy");
    assert_eq!(canopy_rows.len(), 2);
    assert!(untagged.top() < city.top() && city.top() < canopy_rows[0].top());
    assert!(forest.top() < canopy_rows[1].top() && canopy_rows[1].top() < integrated.top());
    assert!(canopy_rows[0].left() > city.left());
    for key in [
        Key::Home,
        Key::ArrowDown,
        Key::ArrowDown,
        Key::ArrowDown,
        Key::ArrowDown,
        Key::Enter,
    ] {
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }
    assert_eq!(layer(&app, uid)["tilesetDefUid"].as_i64(), canopy);
}

#[test]
fn layer_tileset_pickers_create_internal_icons_once_and_undo_assignment() {
    let (mut app, ctx, tiles) = editor("Tiles");
    let p = app.project.as_mut().unwrap();
    let intgrid = p.add_layer("Collision", "IntGrid", 16).unwrap();
    let auto = p.add_layer("Generated", "AutoLayer", 16).unwrap();
    p.update_layer_properties(auto, &json!({"autoSourceLayerDefUid":intgrid}))
        .unwrap();
    let before = p.json.clone();

    let select = |app: &mut EditorApp, uid, placeholder| {
        app.definition_uid = Some(uid);
        frame(app, &ctx, vec![]);
        let closed = frame(app, &ctx, vec![]);
        click(app, &ctx, label(&closed, placeholder).center());
        let opened = frame(app, &ctx, vec![]);
        let icons = labels(&opened, "⚙ Internal icons");
        assert_eq!(icons.len(), 1);
        click(app, &ctx, icons[0].center());
        layer(app, uid)["tilesetDefUid"].as_i64().unwrap()
    };

    let atlas = select(&mut app, tiles, "-- Tileset required --");
    assert!(app.last_chance.is_none());
    let p = app.project.as_ref().unwrap();
    assert_eq!(p.tileset(atlas).unwrap()["embedAtlas"], "LdtkIcons");
    assert_eq!(p.tileset(atlas).unwrap()["tileGridSize"], 16);
    assert!(p.tileset(atlas).unwrap()["relPath"].is_null());
    let assigned = p.json.clone();
    assert_eq!(select(&mut app, auto, "-- Select a tileset --"), atlas);
    assert!(app.last_chance.is_none());
    let p = app.project.as_mut().unwrap();
    assert_eq!(p.json["defs"]["tilesets"].as_array().unwrap().len(), 1);
    assert!(app.history.undo(p));
    assert_eq!(p.json, assigned);
    assert!(app.history.undo(p));
    assert_eq!(p.json, before);

    p.add_auto_rule_group(auto, "Walls").unwrap();
    let with_rules = p.json.clone();
    select(&mut app, auto, "-- Select a tileset --");
    assert_eq!(
        app.last_chance
            .as_ref()
            .map(|(message, _)| message.as_str()),
        Some("Changed auto-layer tileset")
    );
    app.restore_last_chance();
    assert_eq!(app.project.as_ref().unwrap().json, with_rules);
}

#[test]
fn layer_ui_tags_create_and_rename_shared_tags_only_in_layer_ui() {
    let (mut app, ctx, uid) = editor("IntGrid");
    let p = app.project.as_mut().unwrap();
    let other = p.add_layer("Other", "Entities", 16).unwrap();
    let entity = p.add_entity_def("Actor", 16, 16).unwrap();
    p.update_layer_properties(
        other,
        &json!({"uiFilterTags":["shared"],"requiredTags":["shared"]}),
    )
    .unwrap();
    p.update_entity_definition(entity, &json!({"tags":["shared"]}))
        .unwrap();

    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&output, "(No tag)").center());
    frame(&mut app, &ctx, vec![]);
    type_focused(&mut app, &ctx, "shared");
    assert_eq!(layer(&app, uid)["uiFilterTags"], json!(["shared"]));

    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&output, "shared").center());
    frame(&mut app, &ctx, vec![]);
    type_focused(&mut app, &ctx, "other area");
    assert_eq!(layer(&app, uid)["uiFilterTags"], json!(["shared"]));
    let output = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&output, "Rename everywhere").center());
    assert_eq!(layer(&app, uid)["uiFilterTags"], json!(["other_area"]));
    assert_eq!(layer(&app, other)["uiFilterTags"], json!(["other_area"]));
    assert_eq!(layer(&app, other)["requiredTags"], json!(["shared"]));
    assert_eq!(
        app.project.as_ref().unwrap().json["defs"]["entities"][0]["tags"],
        json!(["shared"])
    );
}

#[test]
fn entity_layer_tag_filters_recall_entity_tags_and_exclude_either_current_filter() {
    let (mut app, ctx, uid) = editor("Entities");
    let p = app.project.as_mut().unwrap();
    let other = p.add_layer("Other", "Entities", 16).unwrap();
    let entity = p.add_entity_def("Actor", 16, 16).unwrap();
    p.update_layer_properties(uid, &json!({"excludedTags":["excluded_now"]}))
        .unwrap();
    p.update_layer_properties(
        other,
        &json!({"excludedTags":["from_layer"],"uiFilterTags":["layer_ui"]}),
    )
    .unwrap();
    p.update_entity_definition(entity, &json!({"tags":["from_entity","excluded_now"]}))
        .unwrap();

    let output = frame(&mut app, &ctx, vec![]);
    let row = label(&output, "Required tags");
    let empty = labels(&output, "(No tag)")
        .into_iter()
        .find(|rect| (rect.center().y - row.center().y).abs() < 8.)
        .unwrap();
    click(&mut app, &ctx, empty.center());
    let output = frame(&mut app, &ctx, vec![]);
    label(&output, "FROM_LAYER");
    assert!(labels(&output, "LAYER_UI").is_empty());
    assert!(labels(&output, "EXCLUDED_NOW").is_empty());
    assert_eq!(labels(&output, "excluded_now").len(), 1);
    click(&mut app, &ctx, label(&output, "FROM_ENTITY").center());
    assert_eq!(layer(&app, uid)["requiredTags"], json!(["from_entity"]));
    assert_eq!(layer(&app, uid)["excludedTags"], json!(["excluded_now"]));
}

#[test]
fn rules_open_as_a_compact_panel_with_the_current_layer_and_render_toggle() {
    let (mut app, ctx, uid) = editor("IntGrid");
    let p = app.project.as_mut().unwrap();
    let atlas = p.ensure_embedded_icons().unwrap();
    p.update_layer_properties(uid, &json!({"tilesetDefUid":atlas}))
        .unwrap();
    p.add_auto_rule_group(uid, "Floor rules").unwrap();
    app.panel = Some(Panel::Rules);
    let run = |app: &mut EditorApp, events| {
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
                assert!(app.source_panel(ctx, Panel::Rules));
            },
        )
    };
    run(&mut app, vec![]);
    let out = run(&mut app, vec![]);
    assert_eq!(source_panel_width(Panel::Rules), 356.);
    label(&out, "RULES: TESTLAYER");
    label(&out, "RANDOMIZE");
    assert_eq!(labels(&out, "Floor rules").len(), 1);
    assert!(labels(&out, "Pattern size").is_empty());
    let before = app.project.as_ref().unwrap().json.clone();
    let pos = label(&out, "Render").center();
    for pressed in [true, false] {
        run(
            &mut app,
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
    assert!(!app.render_auto);
    assert_eq!(app.project.as_ref().unwrap().json, before);
}

#[test]
fn intgrid_palette_and_auto_tiles_share_the_same_section() {
    let (mut app, ctx, uid) = editor("IntGrid");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(
            uid,
            &json!({
                "intGridValuesGroups":[{"uid":1,"identifier":"Terrain","color":null}],
                "intGridValues":[
                    {"value":1,"identifier":"Floor","color":"#263D45","groupUid":0,"tile":null},
                    {"value":2,"identifier":"Ledge","color":"#775A42","groupUid":1,"tile":null}
                ]
            }),
        )
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let section = label(&out, "INTGRID SETTINGS");
    assert!(section.top() < label(&out, "Values interface icons").top());
    assert!(label(&out, "Grid values").top() < label(&out, "Terrain").top());
    assert!(label(&out, "Terrain").top() < label(&out, "Auto-layer tileset").top());
    assert!(labels(&out, "AUTOLAYER SETTINGS").is_empty());
    let before = app.project.as_ref().unwrap().json.clone();
    click(&mut app, &ctx, section.center());
    let out = frame(&mut app, &ctx, vec![]);
    assert!(labels(&out, "Grid values").is_empty());
    assert!(labels(&out, "Auto-layer tileset").is_empty());
    assert_eq!(app.project.as_ref().unwrap().json, before);
}

#[test]
fn intgrid_icon_tileset_picker_groups_atlases_and_clears_old_icons() {
    let (mut app, ctx, uid) = editor("IntGrid");
    let project = app.project.as_mut().unwrap();
    let icons = project.ensure_embedded_icons().unwrap();
    let replacement = project
        .add_tileset(Path::new("Replacement.png"), 16, 32, 32)
        .unwrap();
    project
        .update_tileset_definition(replacement, &json!({"tags":["icons"]}))
        .unwrap();
    project
        .update_layer_properties(
            uid,
            &json!({"intGridValues":[
                {"value":1,"color":"#123456","identifier":"Floor","groupUid":0,
                 "tile":{"tilesetUid":icons,"x":0,"y":0,"w":16,"h":16}}
            ]}),
        )
        .unwrap();
    let before = project.json.clone();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&out, "⚙ Internal icons").center());
    let opened = frame(&mut app, &ctx, vec![]);
    assert!(label(&opened, "Untagged").top() < label(&opened, "icons").top());
    click(&mut app, &ctx, label(&opened, "Replacement").center());
    let closed = frame(&mut app, &ctx, vec![]);
    label(&closed, "Replacement");
    assert!(layer(&app, uid)["intGridValues"][0]["tile"].is_null());
    let project = app.project.as_mut().unwrap();
    assert!(app.history.undo(project));
    assert_eq!(project.json, before);
}

#[test]
fn painted_intgrid_value_removal_refreshes_current_linked_layer_and_can_be_undone() {
    let (mut app, ctx, uid) = editor("IntGrid");
    let key = app.selected.unwrap();
    let project = app.project.as_mut().unwrap();
    let linked = project.add_layer("Linked", "AutoLayer", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("Atlas.png"), 16, 16, 16)
        .unwrap();
    project
        .update_layer_properties(
            linked,
            &json!({
                "autoSourceLayerDefUid":uid,"tilesetDefUid":atlas,
                "autoRuleGroups":[{"uid":80,"active":true,"rules":[
                    {"uid":81,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1}
                ]}]
            }),
        )
        .unwrap();
    let other = project.add_level(None, "Other", 32, 32).unwrap();
    for level in [key, other] {
        project.paint_int_grid(level, 1, 0, 0, 1).unwrap();
        assert_eq!(
            project.layer(level, 0).unwrap()["autoLayerTiles"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    fn delete_button(shape: &egui::Shape) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Rect(rect)
                if rect.fill == Color32::from_rgb(121, 17, 5)
                    && rect.rect.size().max_elem() < 40. =>
            {
                Some(rect.rect.center())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(delete_button),
            _ => None,
        }
    }
    let delete = out
        .shapes
        .iter()
        .find_map(|shape| delete_button(&shape.shape))
        .expect("value delete button");
    let before = app.project.as_ref().unwrap().json.clone();
    click(&mut app, &ctx, delete);
    let confirm = frame(&mut app, &ctx, vec![]);
    assert_eq!(app.project.as_ref().unwrap().json, before);
    click(&mut app, &ctx, label(&confirm, "CANCEL").center());
    frame(&mut app, &ctx, vec![]);
    assert_eq!(app.project.as_ref().unwrap().json, before);
    click(&mut app, &ctx, delete);
    let confirm = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&confirm, "CONFIRM").center());
    assert_eq!(layer(&app, uid)["intGridValues"], json!([]));
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.layer(key, 1).unwrap()["intGridCsv"][0], 0);
    assert_eq!(project.layer(other, 1).unwrap()["intGridCsv"][0], 0);
    assert_eq!(project.layer(key, 0).unwrap()["autoLayerTiles"], json!([]));
    assert_eq!(
        project.layer(other, 0).unwrap()["autoLayerTiles"],
        before["levels"][other.level]["layerInstances"][0]["autoLayerTiles"]
    );
    assert!(app.last_chance.is_some());
    app.restore_last_chance();
    assert_eq!(app.project.as_ref().unwrap().json, before);
}
#[test]
fn deleting_intgrid_group_updates_own_and_linked_rules_in_current_level() {
    let (mut app, ctx, source) = editor("IntGrid");
    let key = app.selected.unwrap();
    let project = app.project.as_mut().unwrap();
    let linked = project.add_layer("Linked", "AutoLayer", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("Atlas.png"), 16, 32, 16)
        .unwrap();
    project
        .update_layer_properties(
            source,
            &json!({
                "intGridValuesGroups":[{"uid":1,"identifier":"Terrain"}],
                "intGridValues":[{"value":1,"color":"#263D45","groupUid":1}]
            }),
        )
        .unwrap();
    for (uid, group) in [(source, 80), (linked, 90)] {
        project
            .update_layer_properties(
                uid,
                &json!({
                    "autoSourceLayerDefUid": (uid == linked).then_some(source),
                    "tilesetDefUid":atlas,
                    "autoRuleGroups":[{"uid":group,"active":true,"rules":[
                        {"uid":group+1,"active":true,"size":1,"pattern":[2000],"tileRectsIds":[[0]],"chance":1},
                        {"uid":group+2,"active":true,"size":1,"pattern":[1000],"tileRectsIds":[[1]],"chance":1}
                    ]}]
                }),
            )
            .unwrap();
    }
    let other = project.add_level(None, "Other", 32, 32).unwrap();
    let tiles = |project: &Project, key: LevelKey| {
        [0, 1].map(|index| project.layer(key, index).unwrap()["autoLayerTiles"][0]["t"].clone())
    };
    for level in [key, other] {
        project.paint_int_grid(level, 1, 0, 0, 1).unwrap();
        assert_eq!(tiles(project, level), [0, 0]);
    }
    let before = project.json.clone();
    let action = IntGridRemoval::Group(1);
    frame(&mut app, &ctx, vec![]);
    app.int_grid_removal = Some((source, action));
    frame(&mut app, &ctx, vec![]);
    let confirm = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&confirm, "CANCEL").center());
    assert_eq!(app.project.as_ref().unwrap().json, before);

    app.int_grid_removal = Some((source, action));
    frame(&mut app, &ctx, vec![]);
    let confirm = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&confirm, "CONFIRM").center());
    assert_eq!(layer(&app, source)["intGridValuesGroups"], json!([]));
    assert_eq!(layer(&app, source)["intGridValues"][0]["groupUid"], 0);
    let project = app.project.as_ref().unwrap();
    for level in [key, other] {
        assert_eq!(project.layer(level, 1).unwrap()["intGridCsv"][0], 1);
    }
    assert_eq!(tiles(project, key), [1, 1]);
    assert_eq!(tiles(project, other), [0, 0]);
    assert!(!app.pending_auto_layer_rebuild);
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(app.project.as_ref().unwrap().json, before);
}

#[test]
fn source_layer_groups_and_type_options_do_not_mutate_on_open() {
    for kind in ["Tiles", "IntGrid", "AutoLayer", "Entities"] {
        let (mut app, ctx, _) = editor(kind);
        let before = app.project.as_ref().unwrap().json.clone();
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        assert!(label(&out, "GENERAL LAYER SETTINGS").top() < label(&out, "DISPLAY OPTIONS").top());
        label(&out, "Color in layers list");
        label(&out, "UI tags");
        label(&out, "Show in world view");
        label(&out, "Allow selections when inactive");
        label(&out, "Asynchronous render");
        assert_eq!(
            !labels(&out, "Hide entity fields when inactive").is_empty(),
            kind == "Entities"
        );
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
}
#[test]
fn entity_layer_world_view_checkbox_updates_the_document_and_can_be_undone() {
    let (mut app, ctx, uid) = editor("Entities");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    assert_eq!(layer(&app, uid)["renderInWorldView"], true);
    click(&mut app, &ctx, label(&out, "Show in world view").center());
    assert_eq!(layer(&app, uid)["renderInWorldView"], false);
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["renderInWorldView"], true);
}

#[test]
fn intgrid_and_entities_async_render_can_be_enabled_and_undone() {
    for kind in ["IntGrid", "Entities"] {
        let (mut app, ctx, uid) = editor(kind);
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        assert_eq!(layer(&app, uid)["useAsyncRender"], false);
        click(&mut app, &ctx, label(&out, "Asynchronous render").center());
        assert_eq!(layer(&app, uid)["useAsyncRender"], true);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(layer(&app, uid)["useAsyncRender"], false);
    }
}

#[test]
fn intgrid_with_linked_auto_layer_keeps_async_render_available() {
    let (mut app, ctx, uid) = editor("IntGrid");
    let project = app.project.as_mut().unwrap();
    project
        .update_layer_properties(uid, &json!({"useAsyncRender":true}))
        .unwrap();
    let auto = project.add_layer("Generated", "AutoLayer", 16).unwrap();
    project
        .update_layer_properties(auto, &json!({"autoSourceLayerDefUid":uid}))
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    label(&out, "Asynchronous render");
    assert_eq!(layer(&app, uid)["useAsyncRender"], true);
}

#[test]
fn source_layer_general_form_displays_uppercase_sections_and_official_tutorial_links() {
    let (mut app, ctx, _) = editor("AutoLayer");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let general = label(&out, "GENERAL LAYER SETTINGS");
    let display = label(&out, "DISPLAY OPTIONS");
    let display_offset = display.center().y - general.center().y;
    assert!((365. ..=371.).contains(&display_offset), "{display_offset}");
    let before = app.project.as_ref().unwrap().json.clone();
    for (text, url) in [
        (
            "Tile layer tutorial",
            "https://ldtk.io/docs/tutorials/tile-layers",
        ),
        (
            "Auto-layer tutorial",
            "https://ldtk.io/docs/tutorials/auto-layers",
        ),
    ] {
        let pos = label(&out, text).center();
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), pointer(true)],
        );
        let clicked = frame(&mut app, &ctx, vec![pointer(false)]);
        assert!(
            clicked
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(
                    command, egui::OutputCommand::OpenUrl(link) if link.url == url
                ))
        );
    }
    assert_eq!(app.project.as_ref().unwrap().json, before);
}

#[test]
fn source_layer_numeric_arrow_keys_commit_one_step_without_blurring() {
    let (mut app, ctx, uid) = editor("Tiles");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(uid, &json!({"pxOffsetX":-7}))
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    };
    click(&mut app, &ctx, label(&out, "37").center());
    frame(&mut app, &ctx, vec![key(Key::ArrowUp)]);
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.38);
    frame(&mut app, &ctx, vec![key(Key::ArrowUp)]);
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.39);

    let out = frame(&mut app, &ctx, vec![]);
    click(&mut app, &ctx, label(&out, "-7").center());
    frame(&mut app, &ctx, vec![key(Key::ArrowDown)]);
    assert_eq!(layer(&app, uid)["pxOffsetX"], -8);
}

#[test]
fn source_layer_opacity_drag_previews_and_commits_once_on_release() {
    let (mut app, ctx, uid) = editor("Tiles");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let start = label(&out, "37").center();
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    let end = start + egui::vec2(25., 0.);
    let dragging = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
    label(&dragging, "47");
    fn has_glow(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Rect(rect) => {
                rect.fill == Color32::from_rgb(213, 105, 50) && rect.blur_width == 8.
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(has_glow),
            _ => false,
        }
    }
    assert!(dragging.shapes.iter().any(|shape| has_glow(&shape.shape)));
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.37);
    frame(&mut app, &ctx, vec![pointer(end, false)]);
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.47);
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["displayOpacity"], 0.37);
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

#[test]
fn preview_154_blocker_picker_adds_and_removes_layer() {
    let (mut app, ctx, uid) = editor("AutoLayer");
    let blocker = app
        .project
        .as_mut()
        .unwrap()
        .add_layer("Blocker", "Tiles", 16)
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    label(&out, "AUTOLAYER SETTINGS");
    click(&mut app, &ctx, label(&out, "-- Add a layer --").center());
    let out = frame(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        labels(&out, "Blocker").last().unwrap().center(),
    );
    assert_eq!(
        layer(&app, uid)["layerUidsPreventingAutoTilingHere"],
        json!([blocker]),
        "status: {}",
        app.status
    );
    let out = frame(&mut app, &ctx, vec![]);
    let row_y = labels(&out, "Blocker").last().unwrap().center().y;
    fn delete_button(shape: &egui::Shape, row_y: f32) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Rect(shape)
                if shape.fill == Color32::from_rgb(235, 78, 0)
                    && shape.rect.y_range().contains(row_y) =>
            {
                Some(shape.rect.center())
            }
            egui::Shape::Vec(children) => children
                .iter()
                .find_map(|shape| delete_button(shape, row_y)),
            _ => None,
        }
    }
    let remove = out
        .shapes
        .iter()
        .find_map(|shape| delete_button(&shape.shape, row_y))
        .unwrap();
    click(&mut app, &ctx, remove);
    assert_eq!(
        layer(&app, uid)["layerUidsPreventingAutoTilingHere"],
        json!([])
    );
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(
        layer(&app, uid)["layerUidsPreventingAutoTilingHere"],
        json!([blocker])
    );
    assert!(app.history.redo(app.project.as_mut().unwrap()));
    assert_eq!(
        layer(&app, uid)["layerUidsPreventingAutoTilingHere"],
        json!([])
    );
}

#[test]
fn preview_154_entity_depth_sort_picker_updates_and_undoes() {
    let (mut app, ctx, uid) = editor("Entities");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        label(&out, "Manual (CTRL-UP/CTRL-DOWN to change an entity depth)").center(),
    );
    let out = frame(&mut app, &ctx, vec![]);
    click(
        &mut app,
        &ctx,
        label(&out, "Y ascending (higher Y in front)").center(),
    );
    assert_eq!(layer(&app, uid)["entityDepthSorting"], "YAscending");
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["entityDepthSorting"], "Manual");
}

#[test]
fn layer_form_aligns_controls_and_separates_visual_options_and_guide_rows() {
    let (mut app, ctx, uid) = editor("IntGrid");
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let general = label(&out, "GENERAL LAYER SETTINGS");
    let base = label(&out, "Base:");
    // Source control starts 168 px into the panel body, and its title is inset 24 px.
    assert!(
        (142. ..=146.).contains(&(base.left() - general.left())),
        "expected the original 160-point label track"
    );
    let hide = label(&out, "Hide in list");
    let world = label(&out, "Show in world view");
    let inactive = label(&out, "Allow selections when inactive");
    let asynchronous = label(&out, "Asynchronous render");
    assert!((hide.left() - inactive.left()).abs() < 1.);
    assert!((world.left() - asynchronous.left()).abs() < 1.);
    assert!((hide.center().y - world.center().y).abs() < 1.);
    assert!((inactive.center().y - asynchronous.center().y).abs() < 1.);
    assert!(world.left() > hide.left() && inactive.top() > hide.bottom());

    let dimensions = label(&out, r#"Optional "guide" grid"#);
    let color = label(&out, "Guide grid color");
    assert!(dimensions.bottom() < color.top());
    assert!(labels(&out, "Opacity:").is_empty());

    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(uid, &json!({"guideColor":"#FF0000"}))
        .unwrap();
    let out = frame(&mut app, &ctx, vec![]);
    let opacity = label(&out, "Opacity:");
    assert!((opacity.center().y - label(&out, "Guide grid color").center().y).abs() < 3.);
    let percentages = labels(&out, "%");
    assert_eq!(
        percentages
            .iter()
            .filter(|p| (p.center().y - opacity.center().y).abs() < 3.)
            .count(),
        1
    );
}

#[test]
fn preview_154_guide_opacity_is_available_with_custom_color_and_undoable() {
    let (mut app, ctx, uid) = editor("Tiles");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(uid, &json!({"guideColor":"#FF0000"}))
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    input(&mut app, &ctx, label(&out, "33").center(), "75");
    assert_eq!(layer(&app, uid)["guideOpacity"], 0.75);
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["guideOpacity"], 0.33);
}

#[test]
fn layer_color_rows_use_source_swatches_and_reset_independently() {
    fn swatch(output: &egui::FullOutput, rgb: Color32) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape) if shape.fill == rgb => Some(shape.rect),
                _ => None,
            })
            .expect("color swatch")
    }

    let (mut app, ctx, uid) = editor("Tiles");
    app.project
        .as_mut()
        .unwrap()
        .update_layer_properties(
            uid,
            &json!({
                "uiColor":"#B47055","guideColor":"#BC4646","guideOpacity":0.53
            }),
        )
        .unwrap();
    frame(&mut app, &ctx, vec![]);
    let out = frame(&mut app, &ctx, vec![]);
    let list = swatch(&out, Color32::from_rgb(180, 112, 85));
    let guide = swatch(&out, Color32::from_rgb(188, 70, 70));
    assert_eq!(list.size(), egui::vec2(56., 20.));
    assert_eq!(guide.size(), list.size());
    assert_eq!(guide.left(), list.left());
    assert!((label(&out, "Opacity:").left() - guide.right() - 8.).abs() < 2.);

    click(&mut app, &ctx, list.right_center() + egui::vec2(35., 0.));
    assert!(layer(&app, uid)["uiColor"].is_null());
    assert_eq!(layer(&app, uid)["guideColor"], "#BC4646");
    let pct = labels(&out, "%")
        .into_iter()
        .find(|rect| (rect.center().y - guide.center().y).abs() < 5.)
        .unwrap();
    click(
        &mut app,
        &ctx,
        egui::pos2(pct.right() + 31., guide.center().y),
    );
    assert!(layer(&app, uid)["guideColor"].is_null());
    assert_eq!(layer(&app, uid)["guideOpacity"], 0.53);
    let out = frame(&mut app, &ctx, vec![]);
    assert!(labels(&out, "Opacity:").is_empty());

    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["guideColor"], "#BC4646");
    assert!(layer(&app, uid)["uiColor"].is_null());
    assert!(app.history.undo(app.project.as_mut().unwrap()));
    assert_eq!(layer(&app, uid)["uiColor"], "#B47055");
}
