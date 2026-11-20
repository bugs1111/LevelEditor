use super::*;
use serde_json::json;

impl EditorApp {
    pub(super) fn draw_brush_preview(&mut self, ui: &egui::Ui, rect: Rect, hovered: bool) {
        if self.world
            || self.brush_preview_waits_for_pointer
            || self.panel == Some(Panel::Level)
            || self.gesture.is_some()
            || self.field_pick.is_some()
            || self.pending.is_some()
            || self.is_busy()
            || self.command_open
            || !hovered
        {
            return;
        }
        let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) else {
            return;
        };
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let world = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        if layer["__type"] == "AutoLayer"
            && self
                .stamp
                .as_ref()
                .is_none_or(|stamp| stamp["type"] != "SelectionGroup")
        {
            let [x, y] = world.map(f32::round);
            let [left, top, right, bottom] = canvas::bounds(project, key, false);
            let ctx = ui.ctx();
            if (left..right).contains(&x)
                && (top..bottom).contains(&y)
                && !ctx.input(|input| input.modifiers.alt || input.key_down(Key::Space))
                && !self.selection_contains_world(world)
                && ctx.output(|output| output.cursor_icon == egui::CursorIcon::Default)
            {
                ctx.set_cursor_icon(egui::CursorIcon::NotAllowed);
            }
            return;
        }
        let local = canvas::world_to_layer(project, key, self.layer, &self.viewport, world);
        let grid = number(layer, "__gridSize", 16.0).max(1.0);
        let cell = local.map(|v| (v / grid).floor() as i32);
        if cell[0] < 0
            || cell[1] < 0
            || cell[0] >= number(layer, "__cWid", 0.0) as i32
            || cell[1] >= number(layer, "__cHei", 0.0) as i32
        {
            return;
        }
        if let Some(group) = self
            .stamp
            .as_ref()
            .filter(|v| v["type"] == "SelectionGroup")
        {
            let origin = canvas::layer_to_world(
                project,
                key,
                self.layer,
                &self.viewport,
                cell.map(|v| v as f32 * grid),
            );
            let delta = std::array::from_fn(|axis| {
                origin[axis] - group["origin"][axis].as_f64().unwrap_or(0.0) as f32
            });
            canvas::draw_group_ghost(
                ui.ctx(),
                ui.painter(),
                rect,
                project,
                &self.viewport,
                key,
                group,
                delta,
                [false; 2],
                canvas::GhostMode::Copy,
                &mut self.assets,
            );
            return;
        }
        self.draw_entity_chain_preview(ui, rect, world);
        if self.selection_group.is_some() {
            return;
        }
        let offset = [
            number(layer, "__pxTotalOffsetX", 0.0),
            number(layer, "__pxTotalOffsetY", 0.0),
        ];
        let origin = [
            offset[0] + cell[0] as f32 * grid,
            offset[1] + cell[1] as f32 * grid,
        ];
        let mut ghost_mode = canvas::GhostMode::Brush;
        let mut clipboard = json!({"gridSize":grid,"width":1,"height":1,"gridTiles":[],"entityInstances":[],"intGridCsv":[]});
        match text(layer, "__type", "") {
            "IntGrid" => clipboard["intGridCsv"] = json!([self.int_value]),
            "Tiles" => {
                let Some(def) = layer["__tilesetDefUid"]
                    .as_i64()
                    .and_then(|uid| project.tileset(uid))
                else {
                    return;
                };
                let uid = def["uid"].as_i64().unwrap_or(-1);
                let ids = self.selected_tile_ids_for(uid);
                if ids.is_empty() {
                    return;
                }
                let random_id = if self.random_tiles {
                    if ui.ctx().input(|i| i.pointer.delta().length_sq() > 0.0)
                        || !self.preview_tile.is_some_and(|id| ids.contains(&id))
                    {
                        self.preview_tile =
                            Some(ids[brush::random_index(&mut self.random_state, ids.len())]);
                    }
                    self.preview_tile
                } else {
                    None
                };
                let columns = number(def, "__cWid", 1.0).max(1.0) as i32;
                let atlas_grid = number(def, "tileGridSize", 16.0);
                let spacing = number(def, "spacing", 0.0);
                let padding = number(def, "padding", 0.0);
                let scale = (atlas_grid / grid).round().max(1.0);
                let tiles=tools::stamp_cells([0,0],self.tile_rect,1,self.flips).into_iter().filter_map(|(dest,src)|{
                    let id=src[0]+src[1]*columns;if !ids.contains(&id){return None;}
                    Some(json!({"px":[dest[0]as f32*grid*scale,dest[1]as f32*grid*scale],"src":[padding+src[0]as f32*(atlas_grid+spacing),padding+src[1]as f32*(atlas_grid+spacing)],"t":id,"f":self.flips,"a":1.0}))
                }).collect::<Vec<_>>();
                clipboard["gridTiles"] = if self.random_tiles {
                    json!(
                        tiles
                            .into_iter()
                            .filter(|t| t["t"].as_i64() == random_id.map(i64::from))
                            .map(|mut t| {
                                t["px"] = json!([0, 0]);
                                t
                            })
                            .collect::<Vec<_>>()
                    )
                } else {
                    json!(tiles)
                };
            }
            "Entities" if self.details => {
                let Some(def) = project.json["defs"]["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|d| d["uid"].as_i64() == self.entity_uid)
                else {
                    return;
                };
                if point_tools::hit_point(project, layer, local).is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                    return;
                }
                if self
                    .entity_index
                    .and_then(|index| layer["entityInstances"].get(index))
                    .is_some_and(|entity| selection_click::entity_is_over(project, entity, local))
                {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    return;
                }
                if let Some((_, entity)) = hit_entity(project, layer, local) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    ghost_mode = canvas::GhostMode::EntityHover {
                        ui_scale: self.settings.float("editorUiScale", 1.),
                    };
                    let mut preview = entity.clone();
                    preview["px"] = json!([
                        entity["px"][0].as_f64().unwrap_or(0.0) - f64::from(cell[0] as f32 * grid),
                        entity["px"][1].as_f64().unwrap_or(0.0) - f64::from(cell[1] as f32 * grid)
                    ]);
                    clipboard["entityInstances"] = json!([preview]);
                } else {
                    if ui
                        .ctx()
                        .output(|output| output.cursor_icon == egui::CursorIcon::Default)
                    {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Cell);
                    }
                    let point =
                        ldtk_core::snapped_entity_position(def, local, grid as i32, self.grid);
                    clipboard["entityInstances"] = json!([{"iid":"@cursor","defUid":def["uid"],"__identifier":def["identifier"],"px":[point[0]as f32-cell[0]as f32*grid,point[1]as f32-cell[1]as f32*grid],"width":def["width"],"height":def["height"],"__pivot":[def["pivotX"],def["pivotY"]],"__tile":def["tileRect"],"__smartColor":def["color"],"fieldInstances":[]}]);
                }
            }
            _ => return,
        }
        let group = json!({"origin":origin,"size":[grid,grid],"layers":[{"layerDefUid":layer["layerDefUid"],"origin":origin,"rect":[cell[0],cell[1],1,1],"clipboard":clipboard,"points":[]}]});
        canvas::draw_group_ghost(
            ui.ctx(),
            ui.painter(),
            rect,
            project,
            &self.viewport,
            key,
            &group,
            [0.0; 2],
            [false; 2],
            ghost_mode,
            &mut self.assets,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_brush_highlights_hovered_bounds_and_labels_only_existing_entities() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let actor = project.add_entity_def("Actor", 32, 24).unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 64, 64).unwrap();
        project.json["defs"]["entities"][0]["color"] = json!("#DC424F");
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.entity_uid = Some(actor);
        app.viewport = Viewport {
            center: [128., 128.],
            zoom: 2.,
        };
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512., 512.));
        let frame = |app: &mut EditorApp, point| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events: vec![egui::Event::PointerMoved(point)],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| app.draw_brush_preview(ui, rect, true));
                },
            )
        };
        let has_label = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(110, 33, 40))
            })
        };
        let hovered = egui::pos2(150., 145.);
        for (mode, vertices) in [
            ("Rectangle", 8),
            ("Ellipse", 24),
            ("Cross", 24),
            ("Tile", 8),
        ] {
            app.project.as_mut().unwrap().json["defs"]["entities"][0]["renderMode"] = json!(mode);
            let output = frame(&mut app, hovered);
            assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Grab);
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Mesh(mesh)
                        if mesh.vertices.len() == vertices * 2
                        && mesh.indices.len() == vertices * 6
                        && mesh.vertices.iter().all(|vertex| vertex.color == theme::ACCENT))
                }),
                "missing {mode} hover outline"
            );
            assert!(has_label(&output));
        }
        app.entity_index = Some(0);
        let selected = frame(&mut app, hovered);
        assert_eq!(selected.platform_output.cursor_icon, egui::CursorIcon::Grab);
        assert!(!has_label(&selected));
        let placement = frame(&mut app, egui::pos2(260., 260.));
        assert_eq!(
            placement.platform_output.cursor_icon,
            egui::CursorIcon::Cell
        );
        assert!(!has_label(&placement));
    }
}
