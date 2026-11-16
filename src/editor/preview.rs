use super::*;
use serde_json::json;

impl EditorApp {
    pub(super) fn draw_brush_preview(&mut self, ui: &egui::Ui, rect: Rect, hovered: bool) {
        if self.world
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
                if let Some((_, entity)) = hit_entity(project, layer, local) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    let mut preview = entity.clone();
                    preview["px"] = json!([
                        entity["px"][0].as_f64().unwrap_or(0.0) - f64::from(cell[0] as f32 * grid),
                        entity["px"][1].as_f64().unwrap_or(0.0) - f64::from(cell[1] as f32 * grid)
                    ]);
                    clipboard["entityInstances"] = json!([preview]);
                } else {
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
            canvas::GhostMode::Brush,
            &mut self.assets,
        );
    }
}
