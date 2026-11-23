use super::{selection_click::Element, *};
use serde_json::json;

impl EditorApp {
    pub(super) fn draw_brush_preview(&mut self, ui: &egui::Ui, rect: Rect, hovered: bool) {
        if self.world
            || self.brush_preview_waits_for_pointer
            || self.stroke_waiting_for_level
            || self.neighbor_level_press
            || self.panel == Some(Panel::Level)
            || self.gesture.is_some()
            || self.picking_field()
            || self.pending.is_some()
            || self.is_busy()
            || self.command_open
            || !hovered
        {
            self.entity_eraser_preview = false;
            return;
        }
        let Some(pointer) = ui.ctx().pointer_hover_pos().filter(|p| rect.contains(*p)) else {
            return;
        };
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let (world, _) =
            self.canvas_pointer_pixels(project, key, rect, pointer, ui.ctx().pixels_per_point());
        if ui.input(|input| input.modifiers.alt && !input.key_down(Key::Space)) {
            self.entity_eraser_preview = false;
            self.draw_pick_preview(ui, rect, world);
            return;
        }
        let Some((project, layer)) = self
            .project
            .as_ref()
            .and_then(|project| project.layer(key, self.layer).map(|layer| (project, layer)))
        else {
            return;
        };
        if !self.canvas_layer_visible(key, self.layer)
            || layer["__type"] == "AutoLayer"
            && self
                .stamp
                .as_ref()
                .is_none_or(|stamp| stamp["type"] != "SelectionGroup")
        {
            self.entity_eraser_preview = false;
            let [x, y] = world;
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
        let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, world);
        let grid = number(layer, "__gridSize", 16.0).max(1.0);
        let cell = local.map(|v| (v / grid).floor() as i32);
        let group = self
            .stamp
            .as_ref()
            .filter(|v| v["type"] == "SelectionGroup");
        let entity_brush = layer["__type"] == "Entities" && group.is_none();
        if !entity_brush {
            self.entity_eraser_preview = false;
        } else if ui.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        }) {
            // LDtk updates the eraser on pointer motion, including after release.
            self.entity_eraser_preview = self.stroke && self.stroke_erase;
        }
        if self.entity_eraser_preview {
            return;
        }
        let inside = if entity_brush {
            entity_tools::placement_in_level(project, key, world)
        } else {
            (0..number(layer, "__cWid", 0.0) as i32).contains(&cell[0])
                && (0..number(layer, "__cHei", 0.0) as i32).contains(&cell[1])
        };
        if !inside {
            return;
        }
        if let Some(group) = group {
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
                    let placement = if self.grid { local } else { world };
                    let point =
                        ldtk_core::snapped_entity_position(def, placement, grid as i32, self.grid);
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

    fn draw_pick_preview(&mut self, ui: &egui::Ui, rect: Rect, world: [f32; 2]) {
        let ctx = ui.ctx();
        if self.selection_contains_world(world) {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
            return;
        }
        let Some((index, cell, element)) = self.generic_element_at(world, self.single_layer) else {
            ctx.set_cursor_icon(egui::CursorIcon::Help);
            return;
        };
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let Some(layer) = project.layer(key, index) else {
            return;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
        let painter = ui.painter().with_clip_rect(rect);
        let grid = number(layer, "__gridSize", 16.).max(1.);
        let local = cell.map(|cell| cell as f32 * grid);
        let offset = [
            number(layer, "__pxTotalOffsetX", 0.),
            number(layer, "__pxTotalOffsetY", 0.),
        ];
        let origin = [local[0] + offset[0], local[1] + offset[1]];
        let on_screen = |local| -> egui::Pos2 {
            let anchor = canvas::layer_to_world(project, key, index, &self.viewport, local);
            self.viewport
                .to_screen(
                    anchor,
                    self.viewport
                        .level_render_canvas_origin(rect, ctx.pixels_per_point()),
                    rect.size().into(),
                )
                .into()
        };
        let screen = on_screen(local);
        let grid_rect = Rect::from_min_max(screen, on_screen(local.map(|axis| axis + grid)));
        let ui_scale = self.settings.float("editorUiScale", 1.);
        let grid_scale = grid_rect.width() / grid;

        let (clipboard, mode, label) = match element {
            Element::Grid if layer["__type"] == "Tiles" => {
                let Some(mut tile) = selection_click::tiles_at_cell(layer, cell).last().cloned()
                else {
                    return;
                };
                tile["px"] = json!([0, 0]);
                tile["a"] = json!(1.);
                (
                    json!({"gridSize": grid, "width": 1, "height": 1, "gridTiles": [tile]}),
                    canvas::GhostMode::Brush,
                    Some(selection_click::tile_stack_label(ctx, layer, cell)),
                )
            }
            Element::Grid => {
                let width = number(layer, "__cWid", 0.) as i32;
                let value = &layer["intGridCsv"][(cell[0] + cell[1] * width) as usize];
                let definition = layer["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid))
                    .and_then(|def| {
                        canvas::values(&def["intGridValues"])
                            .iter()
                            .find(|entry| entry["value"] == *value)
                    });
                let color = definition
                    .and_then(|entry| entry["color"].as_str())
                    .map(theme::color)
                    .unwrap_or(theme::ACCENT);
                let label = definition
                    .and_then(|entry| entry["identifier"].as_str())
                    .map_or_else(|| format!("#{value}"), str::to_owned);
                paint_pick_grid(&painter, grid_rect, grid_scale, color, false);
                paint_pick_label(
                    ctx,
                    &painter,
                    &mut self.assets,
                    screen,
                    grid,
                    self.viewport.zoom,
                    ui_scale,
                    &label,
                    color,
                );
                return;
            }
            Element::Entity(index) => {
                let mut entity = layer["entityInstances"][index].clone();
                entity["px"] = json!([
                    entity["px"][0].as_f64().unwrap_or(0.) - f64::from(local[0]),
                    entity["px"][1].as_f64().unwrap_or(0.) - f64::from(local[1]),
                ]);
                (
                    json!({"gridSize": grid, "width": 1, "height": 1, "entityInstances": [entity]}),
                    canvas::GhostMode::EntityHover { ui_scale },
                    None,
                )
            }
            Element::Point { entity, .. } => {
                let owner = &layer["entityInstances"][entity];
                let definition = canvas::values(&project.json["defs"]["entities"])
                    .iter()
                    .find(|def| def["uid"] == owner["defUid"]);
                if let Some(def) = definition {
                    paint_pick_grid(
                        &painter,
                        grid_rect,
                        grid_scale,
                        canvas::entity_smart_color(project, owner, def, false),
                        false,
                    );
                }
                return;
            }
        };

        let group = json!({
            "origin": origin,
            "size": [grid, grid],
            "layers": [{
                "layerDefUid": layer["layerDefUid"],
                "origin": origin,
                "rect": [cell[0], cell[1], 1, 1],
                "clipboard": clipboard,
            }],
        });
        canvas::draw_group_ghost(
            ctx,
            &painter,
            rect,
            project,
            &self.viewport,
            key,
            &group,
            [0.; 2],
            [false; 2],
            mode,
            &mut self.assets,
        );
        if let Some(label) = label {
            paint_pick_label(
                ctx,
                &painter,
                &mut self.assets,
                screen,
                grid,
                self.viewport.zoom,
                ui_scale,
                &label,
                Color32::WHITE,
            );
        }
    }
}

pub(super) fn paint_pick_grid(
    painter: &egui::Painter,
    rect: Rect,
    scale: f32,
    color: Color32,
    fill: bool,
) {
    for (inset, stroke) in [(2., Color32::BLACK), (0., color)] {
        if fill && inset == 0. {
            painter.rect_filled(rect, 0., color.gamma_multiply(0.35));
        }
        let rect = rect.shrink(inset * scale);
        let mut path = egui::epaint::tessellator::Path::default();
        path.add_line_loop(&[
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ]);
        let mut outline = egui::Mesh::default();
        path.stroke_closed(0., &Stroke::new(2. * scale, stroke).into(), &mut outline);
        painter.add(egui::Shape::mesh(outline));
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_pick_label(
    ctx: &egui::Context,
    painter: &egui::Painter,
    assets: &mut Assets,
    screen: egui::Pos2,
    grid: f32,
    zoom: f32,
    scale: f32,
    label: &str,
    color: Color32,
) {
    let foreground = color.lerp_to_gamma(Color32::WHITE, 0.5);
    let background = color.lerp_to_gamma(Color32::BLACK, 0.5);
    let pad = egui::vec2(3., 1.) * (scale / ctx.pixels_per_point());
    let fallback = (!label.is_ascii()).then(|| {
        painter.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(12. * scale),
            foreground,
        )
    });
    let extent = fallback.as_ref().map_or_else(
        || canvas::bitmap_font::measure_flow(ctx, label, scale, f32::INFINITY),
        |galley| galley.size(),
    );
    let size = extent + pad * 2.;
    let min = screen
        + egui::vec2(
            (grid * 0.5 - size.x / (2. * zoom)).trunc(),
            (-size.y / zoom).trunc(),
        ) * zoom;
    painter.rect_filled(Rect::from_min_size(min, size), 0., background);
    if let Some(galley) = fallback {
        painter.galley(min + pad, galley, foreground);
    } else {
        canvas::bitmap_font::paint(
            ctx,
            painter,
            assets,
            min + pad,
            label,
            foreground,
            scale,
            f32::INFINITY,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_hover_shows_intgrid_entity_and_point_cursors() {
        let mut project = Project::new();
        let ground = project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let definition = project.add_entity_def("Actor", 16, 16).unwrap();
        let point = project
            .add_field_definition(Some(definition), "Target", "Point", false)
            .unwrap();
        let key = project.level_keys()[0];
        project
            .update_definition(
                "layers",
                ground,
                &json!({
                    "intGridValues":[{"value":1,"identifier":null,"color":"#759944","groupUid":0}]
                }),
            )
            .unwrap();
        project.add_entity(key, 0, definition, 80, 88).unwrap();
        project.layer_mut(key, 0).unwrap()["entityInstances"][0]["fieldInstances"][0] = json!({"__identifier":"Target","__type":"Point","__value":{"cx":10,"cy":8},"defUid":point});
        project.paint_int_grid(key, 1, 2, 3, 1).unwrap();
        let entity_color = theme::ACCENT;
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.viewport = Viewport {
            center: [128., 128.],
            zoom: 1.,
        };
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 256.));
        let frame = |app: &mut EditorApp, point| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers: egui::Modifiers::ALT,
                    events: vec![egui::Event::PointerMoved(point)],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| app.draw_brush_preview(ui, rect, true))
                        .inner
                },
            )
        };
        let grid = frame(&mut app, egui::pos2(40., 56.));
        assert_eq!(grid.platform_output.cursor_icon, egui::CursorIcon::Grab);
        let grid_color = Color32::from_rgb(117, 153, 68);
        let has_outline = |output: &egui::FullOutput, color| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                    if mesh.indices.len() == 24 && mesh.vertices.iter().all(|vertex| vertex.color == color))
            })
        };
        assert!(has_outline(&grid, grid_color));
        assert!(
            grid.shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.fill == grid_color.lerp_to_gamma(Color32::BLACK, 0.5)))
        );
        assert_eq!(app.layer, 0);
        app.layer = 1;
        let entity = frame(&mut app, egui::pos2(80., 88.));
        assert_eq!(entity.platform_output.cursor_icon, egui::CursorIcon::Grab);
        assert!(
            entity
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect)
            if rect.fill == entity_color.lerp_to_gamma(Color32::BLACK, 0.5)))
        );
        let point = frame(&mut app, egui::pos2(168., 136.));
        assert_eq!(point.platform_output.cursor_icon, egui::CursorIcon::Grab);
        assert!(has_outline(&point, entity_color));
        assert_eq!(app.layer, 1);
    }

    #[test]
    fn alt_hover_previews_the_picked_tile_stack_across_layers() {
        let mut project = Project::new();
        let tile_layer = project.add_layer("Floor", "Tiles", 16).unwrap();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let atlas = project
            .add_tileset(std::path::Path::new("art.png"), 16, 64, 32)
            .unwrap();
        project.assign_tileset(tile_layer, atlas).unwrap();
        let key = project.level_keys()[0];
        project
            .paint_tile(key, 1, 2, 3, 0, [0, 0], 0, false)
            .unwrap();
        project
            .paint_tile_stacked(key, 1, 2, 3, 1, [16, 0], 3, false, true)
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.viewport = Viewport {
            center: [128., 128.],
            zoom: 1.,
        };
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 256.));
        let frame = |app: &mut EditorApp, point| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers: egui::Modifiers::ALT,
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
        let hovered = frame(&mut app, egui::pos2(40., 56.));
        assert_eq!(hovered.platform_output.cursor_icon, egui::CursorIcon::Grab);
        assert!(
            hovered.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.fill == Color32::from_gray(128) && rect.rect.width() == 53.)
            }),
            "draws the source-sized bitmap label for Tiles 0, 1"
        );
        assert_eq!(
            frame(&mut app, egui::pos2(90., 104.))
                .platform_output
                .cursor_icon,
            egui::CursorIcon::Help
        );
        assert_eq!(app.layer, 0);
    }

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

    #[test]
    fn entity_erase_cursor_changes_on_motion_and_stays_until_the_next_move() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.entity_uid = Some(actor);
        app.fit = false;
        app.viewport.center = [128.; 2];
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 256.));
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) =
                                ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                            app.canvas_controls(ctx, rect, &response);
                            app.draw_brush_preview(ui, rect, response.hovered());
                        });
                },
            )
            .platform_output
            .cursor_icon
        };
        let start = egui::pos2(64., 64.);
        let moved = egui::pos2(65., 64.);
        let right = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![]);
        assert_eq!(
            frame(vec![egui::Event::PointerMoved(start)]),
            egui::CursorIcon::Cell
        );
        assert_eq!(frame(vec![right(start, true)]), egui::CursorIcon::Cell);
        assert_eq!(
            frame(vec![egui::Event::PointerMoved(moved)]),
            egui::CursorIcon::Default
        );
        assert_eq!(frame(vec![right(moved, false)]), egui::CursorIcon::Default);
        assert_eq!(
            frame(vec![egui::Event::PointerMoved(start)]),
            egui::CursorIcon::Cell
        );
    }
}
