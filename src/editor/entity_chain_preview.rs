//! EntityTool's temporary reference link while placing chained entities.
use super::*;

struct ChainPreview<'a> {
    start: [f32; 2],
    end: [f32; 2],
    color: Color32,
    style: &'a str,
}
impl EditorApp {
    fn entity_chain_preview(&self, world: [f32; 2]) -> Option<ChainPreview<'_>> {
        if self.world {
            return None;
        }
        let project = self.project.as_ref()?;
        let key = self.selected?;
        let active = project.layer(key, self.layer)?;
        if active["__type"] != "Entities" {
            return None;
        }
        let definitions = project.json["defs"]["entities"].as_array()?;
        let brush = definitions
            .iter()
            .find(|def| def["uid"].as_i64() == self.entity_uid)?;
        let previous = self.chain_entity.as_deref()?;
        let found = project.entity_by_iid(previous)?;
        let source_key = found.level;
        let source_layer = project.layer(source_key, found.layer_index)?;
        let source = found.entity;
        let source_def = definitions
            .iter()
            .find(|def| def["uid"] == source["defUid"])?;
        let field = source["fieldInstances"]
            .as_array()?
            .iter()
            .find_map(|instance| {
                let field = source_def["fieldDefs"]
                    .as_array()?
                    .iter()
                    .find(|field| field["uid"] == instance["defUid"])?;
                if field["autoChainRef"] != true
                    || !matches!(
                        field["__type"].as_str(),
                        Some("EntityRef" | "Array<EntityRef>")
                    )
                    || (field["allowOutOfLevelRef"] == false && source_key != key)
                {
                    return None;
                }
                let accepted = match text(field, "allowedRefs", "Any") {
                    "OnlySame" => source["defUid"] == brush["uid"],
                    "OnlySpecificEntity" => field["allowedRefsEntityUid"] == brush["uid"],
                    "OnlyTags" => field["allowedRefTags"].as_array().is_some_and(|tags| {
                        brush["tags"]
                            .as_array()
                            .is_some_and(|actual| tags.iter().any(|tag| actual.contains(tag)))
                    }),
                    _ => true,
                };
                accepted.then_some(field)
            })?;
        // World positions are runtime placements (including linear layout). The source temp
        // link itself adds raw layer offsets; it does not apply either layer's parallax.
        let origin = |key| {
            canvas::placements(project, key)
                .into_iter()
                .find(|(k, _)| *k == key)
                .map(|(_, rect)| [rect[0], rect[1]])
        };
        let source_origin = origin(source_key)?;
        let current_origin = origin(key)?;
        let start = std::array::from_fn(|axis| {
            source["px"][axis].as_f64().unwrap_or(0.) as f32 + source_origin[axis]
                - current_origin[axis]
                + number(
                    source_layer,
                    if axis == 0 {
                        "__pxTotalOffsetX"
                    } else {
                        "__pxTotalOffsetY"
                    },
                    0.,
                )
        });
        let end = if let Some((layer, _, selection_click::Element::Entity(index))) =
            self.generic_element_at(world, false)
        {
            let layer = project.layer(key, layer)?;
            let target = &layer["entityInstances"][index];
            let target_def = definitions
                .iter()
                .find(|def| def["uid"] == target["defUid"])?;
            std::array::from_fn(|axis| {
                let pos = target["px"][axis].as_f64().unwrap_or(0.) as f32;
                let attach = if field["editorDisplayMode"] == "RefLinkBetweenCenters" {
                    let pivot = number(target_def, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
                    let size = number(target, if axis == 0 { "width" } else { "height" }, 16.);
                    (pos + (0.5 - pivot) * size + 0.5).floor()
                } else {
                    pos
                };
                attach
                    + number(
                        layer,
                        if axis == 0 {
                            "__pxTotalOffsetX"
                        } else {
                            "__pxTotalOffsetY"
                        },
                        0.,
                    )
            })
        } else {
            let rounded = world.map(f32::round);
            let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, rounded);
            let grid = number(active, "__gridSize", 16.).max(1.);
            std::array::from_fn(|axis| {
                let pos = if self.grid {
                    let pivot = number(brush, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
                    (((local[axis] / grid).floor() + pivot) * grid + 0.5).floor()
                } else {
                    rounded[axis]
                };
                pos + number(
                    active,
                    if axis == 0 {
                        "__pxTotalOffsetX"
                    } else {
                        "__pxTotalOffsetY"
                    },
                    0.,
                )
            })
        };
        Some(ChainPreview {
            start,
            end,
            color: canvas::entity_smart_color(project, source, source_def, true)
                .gamma_multiply(0.33),
            style: text(field, "editorLinkStyle", "CurvedArrow"),
        })
    }
    pub(super) fn draw_entity_chain_preview(&self, ui: &egui::Ui, rect: Rect, world: [f32; 2]) {
        let Some(preview) = self.entity_chain_preview(world) else {
            return;
        };
        let screen = |point| {
            egui::Pos2::from(
                self.viewport
                    .to_screen(point, rect.min.into(), rect.size().into()),
            )
        };
        canvas::draw_entity_ref_link(
            &ui.painter().with_clip_rect(rect),
            screen(preview.start),
            screen(preview.end),
            preview.color,
            preview.style,
            self.viewport.zoom,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const KEY: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    fn editor() -> (EditorApp, i64, i64) {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        p.update_entity_definition(uid, &json!({"pivotX":0.25,"pivotY":0.75,"color":"#204060"}))
            .unwrap();
        let field = p
            .add_field_definition(Some(uid), "Next", "EntityRef", false)
            .unwrap();
        p.add_entity(KEY, 0, uid, 20, 30).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(uid);
        app.chain_entity = app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"]
            [0]["iid"]
            .as_str()
            .map(str::to_owned);
        app.viewport.center = [128., 128.];
        app.viewport.zoom = 1.;
        app.grid = true;
        (app, uid, field)
    }
    #[test]
    fn empty_space_preview_uses_pivot_snap_bright_color_and_source_alpha() {
        let (app, _, _) = editor();
        let preview = app
            .entity_chain_preview([81.2, 98.1])
            .expect("chaining preview");
        assert_eq!(preview.start, [20., 30.]);
        assert_eq!(preview.end, [84., 108.]);
        assert_eq!(preview.style, "CurvedArrow");
        assert_eq!(
            preview.color,
            Color32::from_rgb(143, 159, 175).gamma_multiply(0.33)
        );
    }
    #[test]
    fn source_raw_offset_endpoints_preserve_parallax_and_unsnapped_quirks_across_levels() {
        let (mut app, _, _) = editor();
        let p = app.project.as_mut().unwrap();
        p.move_level(KEY, 100, 200, 0).unwrap();
        let next = p.add_level(None, "Other", 256, 256).unwrap();
        p.move_level(next, 400, 500, 0).unwrap();
        p.layer_mut(KEY, 0).unwrap()["__pxTotalOffsetX"] = json!(7);
        p.layer_mut(KEY, 0).unwrap()["__pxTotalOffsetY"] = json!(9);
        p.layer_mut(next, 0).unwrap()["__pxTotalOffsetX"] = json!(11);
        p.layer_mut(next, 0).unwrap()["__pxTotalOffsetY"] = json!(13);
        p.json["defs"]["layers"][0]["parallaxFactorX"] = json!(0.5);
        p.json["defs"]["layers"][0]["parallaxScaling"] = json!(true);
        app.navigate_level(next, false);
        app.grid = false;
        let preview = app.entity_chain_preview([81.2, 98.7]).unwrap();
        assert_eq!(preview.start, [-273., -261.]);
        assert_eq!(preview.end, [92., 112.]);
    }
    #[test]
    fn predicate_uses_brush_and_current_level_and_hovered_entity_is_not_refiltered() {
        let (mut app, uid, field) = editor();
        let p = app.project.as_mut().unwrap();
        let other = p.add_entity_def("Other", 32, 24).unwrap();
        p.update_entity_definition(other, &json!({"pivotX":0.,"pivotY":0.}))
            .unwrap();
        p.add_entity(KEY, 0, other, 80, 80).unwrap();
        let preview = app.entity_chain_preview([85., 85.]).unwrap();
        assert_eq!(
            preview.end,
            [96., 92.],
            "source attaches to hovered Other despite OnlySame accepting the Actor brush"
        );
        app.project
            .as_mut()
            .unwrap()
            .update_field_definition(field, &json!({"editorDisplayMode":"RefLinkBetweenPivots"}))
            .unwrap();
        assert_eq!(
            app.entity_chain_preview([85., 85.]).unwrap().end,
            [80., 80.]
        );
        app.entity_uid = Some(other);
        assert!(app.entity_chain_preview([140., 140.]).is_none());
        app.entity_uid = Some(uid);
        let p = app.project.as_mut().unwrap();
        p.update_field_definition(field, &json!({"allowOutOfLevelRef":false}))
            .unwrap();
        let next = p.add_level(None, "NextLevel", 256, 256).unwrap();
        app.navigate_level(next, false);
        assert!(app.entity_chain_preview([100., 100.]).is_none());
    }
    #[test]
    fn preview_is_absent_when_chain_brush_or_active_entity_tool_is_missing() {
        let (mut app, _, _) = editor();
        app.entity_uid = None;
        assert!(app.entity_chain_preview([100., 100.]).is_none());
        let (mut app, _, _) = editor();
        app.chain_entity = Some("removed-entity".into());
        assert!(app.entity_chain_preview([100., 100.]).is_none());
        let (mut app, _, _) = editor();
        app.world = true;
        assert!(app.entity_chain_preview([100., 100.]).is_none());
    }
    #[test]
    fn hover_reuses_visible_cross_layer_pick_policy_and_raw_offsets() {
        let (mut app, uid, _) = editor();
        let p = app.project.as_mut().unwrap();
        let active_uid = p.layer(KEY, 0).unwrap()["layerDefUid"].as_i64().unwrap();
        let target_uid = p.add_layer("OtherActors", "Entities", 16).unwrap();
        let index = p.level(KEY).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"] == target_uid)
            .unwrap();
        p.add_entity(KEY, index, uid, 80, 80).unwrap();
        p.layer_mut(KEY, index).unwrap()["__pxTotalOffsetX"] = json!(10);
        p.layer_mut(KEY, index).unwrap()["__pxTotalOffsetY"] = json!(20);
        app.layer = p.level(KEY).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"] == active_uid)
            .unwrap();
        assert_eq!(
            app.entity_chain_preview([92., 98.]).unwrap().end,
            [94., 96.]
        );
        app.hidden.insert(target_uid);
        assert_eq!(
            app.entity_chain_preview([92., 98.]).unwrap().end,
            [84., 108.]
        );
        app.hidden.clear();
        let def = app.project.as_mut().unwrap().json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["uid"] == target_uid)
            .unwrap();
        def["canSelectWhenInactive"] = json!(false);
        assert_eq!(
            app.entity_chain_preview([92., 98.]).unwrap().end,
            [84., 108.]
        );
    }
    #[test]
    fn smart_field_color_is_used_without_extra_brightening_and_array_refs_chain() {
        let (mut app, uid, field) = editor();
        let p = app.project.as_mut().unwrap();
        p.update_field_definition(field, &json!({"autoChainRef":false}))
            .unwrap();
        let array = p
            .add_field_definition(Some(uid), "Many", "EntityRef", true)
            .unwrap();
        p.update_field_definition(array, &json!({"editorLinkStyle":"DashedLine"}))
            .unwrap();
        let color = p
            .add_field_definition(Some(uid), "Tint", "Color", false)
            .unwrap();
        p.set_field_value(KEY, app.chain_entity.as_deref(), color, json!("#102030"))
            .unwrap();
        let preview = app.entity_chain_preview([160., 160.]).unwrap();
        assert_eq!(
            preview.color,
            Color32::from_rgb(16, 32, 48).gamma_multiply(0.33)
        );
        assert_eq!(preview.style, "DashedLine");
    }
    #[test]
    fn real_brush_preview_draws_every_link_style_before_selection_early_return() {
        let (mut app, _, field) = editor();
        app.selection_group = Some(json!({"type":"SelectionGroup"}));
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512., 512.));
        let pointer = egui::Pos2::from(app.viewport.to_screen(
            [160., 160.],
            rect.min.into(),
            rect.size().into(),
        ));
        let mut distinct = std::collections::HashSet::new();
        for style in [
            "CurvedArrow",
            "StraightArrow",
            "ZigZag",
            "ArrowsLine",
            "DashedLine",
        ] {
            app.project
                .as_mut()
                .unwrap()
                .update_field_definition(field, &json!({"editorLinkStyle":style}))
                .unwrap();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events: vec![egui::Event::PointerMoved(pointer)],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| app.draw_brush_preview(ui, rect, true));
                },
            );
            let links = output
                .shapes
                .iter()
                .filter(|shape| match &shape.shape {
                    egui::Shape::LineSegment { stroke, .. } => {
                        assert!(stroke.color.a() <= 84);
                        true
                    }
                    egui::Shape::Path(path) => {
                        assert!(path.fill.a() <= 84);
                        true
                    }
                    _ => false,
                })
                .collect::<Vec<_>>();
            assert!(
                !links.is_empty(),
                "{style} must draw even with selection present"
            );
            assert!(links.iter().all(|shape| shape.clip_rect == rect));
            distinct.insert(format!("{links:?}"));
        }
        assert_eq!(distinct.len(), 5);
    }
}
