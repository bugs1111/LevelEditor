//! Source single-element picking and selection, shared by Alt-click and tiny rectangles.
use super::*;
use serde_json::json;

pub(super) fn entity_is_over(project: &Project, entity: &Value, local: [f32; 2]) -> bool {
    let Some(def) = project.json["defs"]["entities"]
        .as_array()
        .and_then(|defs| defs.iter().find(|d| d["uid"] == entity["defUid"]))
    else {
        return false;
    };
    let local = local.map(f32::trunc);
    let w = number(entity, "width", 16.);
    let h = number(entity, "height", 16.);
    let x = entity["px"][0].as_f64().unwrap_or(0.) as f32;
    let y = entity["px"][1].as_f64().unwrap_or(0.) as f32;
    if (local[0] - x).abs() > w + 8. || (local[1] - y).abs() > h + 8. {
        return false;
    }
    let pivot = [number(def, "pivotX", 0.), number(def, "pivotY", 0.)];
    let left = (x - w * pivot[0] + 0.5).floor();
    let top = (y - h * pivot[1] + 0.5).floor();
    let hollow = def["hollow"] == true;
    if def["renderMode"] == "Ellipse" {
        let dx2 = (local[0] - (x + (0.5 - pivot[0]) * w + 0.5).floor()).powi(2);
        let dy2 = (local[1] - (y + (0.5 - pivot[1]) * h + 0.5).floor()).powi(2);
        let inside = |pad: f32| {
            let rx2 = (w * 0.5 + pad).powi(2);
            let ry2 = (h * 0.5 + pad).powi(2);
            dx2 * ry2 + dy2 * rx2 <= rx2 * ry2
        };
        if hollow {
            !inside(-4.) && inside(4.)
        } else {
            inside(3.)
        }
    } else {
        let inside = |pad: f32| {
            local[0] >= left - pad
                && local[0] <= left + w + pad
                && local[1] >= top - pad
                && local[1] <= top + h + pad
        };
        if hollow {
            inside(4.) && !inside(-4.)
        } else {
            inside(3.)
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Element {
    Grid,
    Entity(usize),
    Point {
        entity: usize,
        field: i64,
        index: usize,
    },
}
pub(super) fn tiles_at_cell(layer: &Value, cell: [i32; 2]) -> impl Iterator<Item = &Value> {
    let grid = number(layer, "__gridSize", 16.) as i64;
    let px = cell.map(|axis| i64::from(axis) * grid);
    layer["gridTiles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |tile| {
            tile["px"][0].as_i64() == Some(px[0]) && tile["px"][1].as_i64() == Some(px[1])
        })
}

pub(super) fn layer_hit(
    project: &Project,
    layer: &Value,
    local: [f32; 2],
    cell: [i32; 2],
) -> Option<Element> {
    match text(layer, "__type", "") {
        "IntGrid" => {
            let width = number(layer, "__cWid", 0.) as i32;
            let height = number(layer, "__cHei", 0.) as i32;
            (cell[0] >= 0
                && cell[1] >= 0
                && cell[0] < width
                && cell[1] < height
                && layer["intGridCsv"][(cell[0] + cell[1] * width) as usize]
                    .as_i64()
                    .unwrap_or(0)
                    > 0)
            .then_some(Element::Grid)
        }
        "Tiles" => tiles_at_cell(layer, cell).next().map(|_| Element::Grid),
        "Entities" => {
            let mut hit = None;
            let def = layer["layerDefUid"]
                .as_i64()
                .and_then(|uid| project.layer_def(uid))
                .unwrap_or(&Value::Null);
            for index in canvas::entity_display_order(layer, def) {
                let entity = &layer["entityInstances"][index];
                if entity_is_over(project, entity, local) {
                    hit = Some(Element::Entity(index));
                } else {
                    for field in entity["fieldInstances"].as_array().into_iter().flatten() {
                        let values = match field["__type"].as_str() {
                            Some("Point") => std::slice::from_ref(&field["__value"]),
                            Some("Array<Point>") => field["__value"]
                                .as_array()
                                .map(Vec::as_slice)
                                .unwrap_or_default(),
                            _ => continue,
                        };
                        for (point, value) in values.iter().enumerate() {
                            if value["cx"].as_i64() == Some(i64::from(cell[0]))
                                && value["cy"].as_i64() == Some(i64::from(cell[1]))
                                && let Some(uid) = field["defUid"].as_i64()
                            {
                                hit = Some(Element::Point {
                                    entity: index,
                                    field: uid,
                                    index: point,
                                });
                            }
                        }
                    }
                }
            }
            hit
        }
        _ => None,
    }
}

impl EditorApp {
    pub(super) fn generic_element_at(
        &self,
        world: [f32; 2],
        active_only: bool,
    ) -> Option<(usize, [i32; 2], Element)> {
        let key = self.selected?;
        let project = self.project.as_ref()?;
        let layers = project.level(key)?["layerInstances"].as_array()?;
        let mut order = project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| layers.iter().position(|l| l["layerDefUid"] == d["uid"]))
            .filter(|index| !active_only || *index == self.layer)
            .collect::<Vec<_>>();
        if self.single_layer
            && let Some(pos) = order.iter().position(|i| *i == self.layer)
        {
            let active = order.remove(pos);
            order.insert(0, active);
        }
        order.into_iter().find_map(|index| {
            let layer = &layers[index];
            let uid = layer["layerDefUid"].as_i64()?;
            let def = project.layer_def(uid)?;
            if layer["visible"] == false
                || self.hidden.contains(&uid)
                || (index != self.layer
                    && (def["canSelectWhenInactive"] == false
                        || number(def, "inactiveOpacity", 1.) <= 0.))
                || (!self.details
                    && (layer["__type"] == "Entities"
                        || layer["__type"] == "IntGrid" && def["tilesetDefUid"].is_null()))
            {
                return None;
            }
            let local = canvas::pointer_to_layer(
                project,
                key,
                index,
                &self.viewport,
                world.map(f32::round),
            );
            let grid = number(layer, "__gridSize", 16.).max(1.);
            let cell = local.map(|v| (v / grid).floor() as i32);
            layer_hit(project, layer, local, cell).map(|element| (index, cell, element))
        })
    }
    pub(super) fn element_selection_group(
        &self,
        project: &Project,
        key: LevelKey,
        layer_index: usize,
        cell: [i32; 2],
        element: Element,
    ) -> ldtk_core::Result<Value> {
        let layer = project.layer(key, layer_index).unwrap();
        let grid = number(layer, "__gridSize", 16.);
        let mut capture_cell = cell;
        let mut local_min = [cell[0] as f32 * grid, cell[1] as f32 * grid];
        let mut local_max = [local_min[0] + grid, local_min[1] + grid];
        if let Element::Entity(index) = element {
            let e = &layer["entityInstances"][index];
            let w = number(e, "width", 16.);
            let h = number(e, "height", 16.);
            let px = [
                e["px"][0].as_f64().unwrap_or(0.) as f32,
                e["px"][1].as_f64().unwrap_or(0.) as f32,
            ];
            let pivot = [
                e["__pivot"][0].as_f64().unwrap_or(0.) as f32,
                e["__pivot"][1].as_f64().unwrap_or(0.) as f32,
            ];
            capture_cell = std::array::from_fn(|i| {
                ((px[i] - f32::from(pivot[i] == 1.)) / grid).trunc() as i32
            });
            local_min = [
                (px[0] - w * pivot[0] + 0.5).floor(),
                (px[1] - h * pivot[1] + 0.5).floor(),
            ];
            local_max = [local_min[0] + w, local_min[1] + h];
        }
        let a = canvas::layer_to_world(project, key, layer_index, &self.viewport, local_min);
        let b = canvas::layer_to_world(project, key, layer_index, &self.viewport, local_max);
        let bounds = [
            a[0].floor() as i32,
            a[1].floor() as i32,
            (b[0] - a[0]).ceil().max(1.) as i32,
            (b[1] - a[1]).ceil().max(1.) as i32,
        ];
        let geometries = self.selection_geometries(key);
        let regions = json!([{"layerIndex":layer_index,"rect":[capture_cell[0],capture_cell[1],1,1],"geometry":geometries[layer_index]}]);
        project
            .copy_selection_group_from_regions(key, &regions, bounds)
            .and_then(|mut group| {
                let entry = &mut group["layers"][0];
                match element {
                    Element::Grid => {
                        entry["points"] = json!([]);
                    }
                    Element::Entity(index) => {
                        let mut entity = layer["entityInstances"][index].clone();
                        entity["px"] = json!([
                            entity["px"][0].as_i64().unwrap_or(0)
                                - i64::from(capture_cell[0]) * grid as i64,
                            entity["px"][1].as_i64().unwrap_or(0)
                                - i64::from(capture_cell[1]) * grid as i64
                        ]);
                        entry["clipboard"]["entityInstances"] = json!([entity]);
                        entry["points"] = json!([]);
                    }
                    Element::Point {
                        entity,
                        field,
                        index,
                    } => {
                        let owner = &layer["entityInstances"][entity];
                        entry["clipboard"]["entityInstances"] = json!([]);
                        entry["points"]
                            .as_array_mut()
                            .expect("selection points")
                            .retain(|p| {
                                p["entityIid"] == owner["iid"]
                                    && p["fieldDefUid"] == field
                                    && p["arrayIndex"] == index
                            });
                    }
                }
                group["originalRects"] = json!([]);
                group["emptySpace"] = json!(false);
                project.refresh_selection_group_geometry(key, &group, &geometries)
            })
    }

    pub(super) fn select_element_at(
        &mut self,
        world: [f32; 2],
        copy: bool,
        press: Option<egui::Pos2>,
    ) -> bool {
        let (Some(key), Some(project)) = (self.selected, &self.project) else {
            return false;
        };
        let Some(layers) = project
            .level(key)
            .and_then(|level| level["layerInstances"].as_array())
        else {
            return false;
        };
        let Some((layer_index, cell, element)) = self.generic_element_at(world, false) else {
            self.selection = None;
            self.selection_group = None;
            self.entity_index = None;
            self.chain_entity = None;
            if self.panel == Some(Panel::Entity) {
                self.panel = None;
            }
            return false;
        };
        let layer = &layers[layer_index];
        let group = match self.element_selection_group(project, key, layer_index, cell, element) {
            Ok(group) => group,
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
                return true;
            }
        };
        self.layer = layer_index;
        self.entity_index = None;
        self.chain_entity = None;
        match element {
            Element::Grid => {
                let grid = number(layer, "__gridSize", 16.);
                if layer["__type"] == "IntGrid" {
                    let width = number(layer, "__cWid", 1.) as i32;
                    self.int_value = layer["intGridCsv"][(cell[0] + cell[1] * width) as usize]
                        .as_i64()
                        .unwrap_or(0) as i32;
                } else if let Some(tile) = layer["gridTiles"].as_array().and_then(|ts| {
                    ts.iter().rev().find(|t| {
                        t["px"][0].as_f64() == Some(f64::from(cell[0] as f32 * grid))
                            && t["px"][1].as_f64() == Some(f64::from(cell[1] as f32 * grid))
                    })
                }) {
                    let uid = layer["__tilesetDefUid"].as_i64().unwrap_or(-1);
                    let id = tile["t"].as_i64().unwrap_or(0) as i32;
                    let flips = tile["f"].as_i64().unwrap_or(0) as i32;
                    let ctx = self.repaint.clone();
                    if let Some(ctx) = &ctx {
                        self.restore_canvas_atlas_memory(ctx, uid);
                    }
                    let current = self.selected_tile_ids_for(uid);
                    let saved = self.project.as_ref().and_then(|project| {
                        let saved = library::saved_selection_for(project.tileset(uid)?, id)?;
                        let ids = library::saved_selection_ids(saved);
                        (!ids.iter().any(|id| current.contains(id)))
                            .then_some((ids, saved["mode"] == "Random"))
                    });
                    let (ids, random) = saved.unwrap_or_else(|| (vec![id], self.random_tiles));
                    if self.set_atlas_selection(uid, ids) {
                        self.random_tiles = random;
                        self.flips = flips;
                        if let Some(ctx) = &ctx {
                            self.remember_canvas_atlas_memory(ctx, uid);
                            palette_popout::request_focus(ctx, uid, false);
                        }
                    }
                }
                if self.panel == Some(Panel::Entity) {
                    self.panel = None;
                }
            }
            Element::Entity(index) | Element::Point { entity: index, .. } => {
                self.entity_index = Some(index);
                self.entity_uid = layer["entityInstances"][index]["defUid"].as_i64();
                self.panel = Some(Panel::Entity);
            }
        }
        self.set_selection_group(group);
        if let Some(press) = press {
            let region = self.selection.unwrap_or([cell[0], cell[1], 1, 1]);
            self.begin_selection_move(key, region, cell, world, press, copy);
        }
        self.bleep_selected_element(key, layer_index, cell, element);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const K: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    fn app(project: Project) -> EditorApp {
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.fit = false;
        app
    }
    #[test]
    fn alt_pick_top_layer_creates_a_group_that_drags_and_deletes() {
        let mut p = Project::new();
        p.add_layer("Bottom", "IntGrid", 16).unwrap();
        p.add_layer("Top", "IntGrid", 16).unwrap();
        p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
        p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
        let mut a = app(p);
        a.layer = 1;
        assert!(a.select_element_at([4., 4.], false, Some(egui::Pos2::ZERO)));
        assert_eq!(a.layer, 0);
        assert_eq!(a.selection_group.as_ref().unwrap()["elementCount"], 1);
        assert_eq!(
            a.selection_group.as_ref().unwrap()["originalRects"],
            json!([])
        );
        assert_eq!(a.int_value, 1);
        if let Some(tools::Gesture::SelectionMove {
            world_end, moved, ..
        }) = &mut a.gesture
        {
            *world_end = [36., 4.];
            *moved = true;
        } else {
            panic!("single pick must immediately start selection movement");
        }
        a.finish_gesture();
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["intGridCsv"][2],
            1
        );
        a.delete_selection();
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["intGridCsv"][2],
            0
        );
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 1).unwrap()["intGridCsv"][0],
            1
        );
    }
    #[test]
    fn entity_click_selects_only_last_hit_even_when_pivots_share_a_cell() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 64, 64).unwrap();
        p.update_entity_definition(def, &json!({"pivotX":0.,"pivotY":0.}))
            .unwrap();
        p.add_entity(K, 0, def, 0, 0).unwrap();
        p.add_entity(K, 0, def, 0, 0).unwrap();
        let expected = p.layer(K, 0).unwrap()["entityInstances"][1]["iid"].clone();
        let mut a = app(p);
        assert!(a.select_element_at([40., 40.], false, None));
        let g = a.selection_group.as_ref().unwrap();
        assert_eq!(g["elementCount"], 1);
        assert_eq!(
            g["layers"][0]["clipboard"]["entityInstances"][0]["iid"],
            expected
        );
        assert_eq!(
            g["layers"][0]["clipboard"]["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(g["layers"][0]["points"].as_array().unwrap().is_empty());
        assert_eq!(a.entity_index, Some(1));
        assert!(a.panel == Some(Panel::Entity));
        a.delete_selection();
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn a_later_owners_point_wins_over_an_earlier_body_and_selects_only_that_point() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 32, 32).unwrap();
        p.update_entity_definition(def, &json!({"pivotX":0.,"pivotY":0.}))
            .unwrap();
        let field = p
            .add_field_definition(Some(def), "Target", "Point", false)
            .unwrap();
        p.add_entity(K, 0, def, 0, 0).unwrap();
        p.add_entity(K, 0, def, 80, 80).unwrap();
        let id = p.layer(K, 0).unwrap()["entityInstances"][1]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_value(K, Some(&id), field, json!({"cx":1,"cy":1}))
            .unwrap();
        let mut a = app(p);
        assert!(a.select_element_at([20., 20.], false, None));
        let g = a.selection_group.as_ref().unwrap();
        assert_eq!(g["elementCount"], 1);
        assert!(
            g["layers"][0]["clipboard"]["entityInstances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(g["layers"][0]["points"][0]["entityIid"], id);
        assert_eq!(a.entity_index, Some(1));
        assert!(a.panel == Some(Panel::Entity));
        a.delete_selection();
        let entities = &a.project.as_ref().unwrap().layer(K, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 2);
        assert!(entities[1]["fieldInstances"][0]["__value"].is_null());
    }
    #[test]
    fn picking_from_another_atlas_preserves_the_sample_when_its_palette_opens() {
        let mut project = Project::new();
        let art = project.add_layer("Art", "Tiles", 16).unwrap();
        let other = project.add_layer("Other", "Tiles", 16).unwrap();
        let atlas = project
            .add_tileset(std::path::Path::new("art.png"), 16, 64, 32)
            .unwrap();
        let other_atlas = project
            .add_tileset(std::path::Path::new("other.png"), 16, 64, 32)
            .unwrap();
        project.assign_tileset(art, atlas).unwrap();
        project.assign_tileset(other, other_atlas).unwrap();
        project
            .update_tileset_definition(
                atlas,
                &json!({"savedSelections":[{"ids":[1,2],"mode":"Random"}]}),
            )
            .unwrap();
        let layers = project.level(K).unwrap()["layerInstances"]
            .as_array()
            .unwrap();
        let art_index = layers
            .iter()
            .position(|layer| layer["layerDefUid"] == art)
            .unwrap();
        let other_index = layers
            .iter()
            .position(|layer| layer["layerDefUid"] == other)
            .unwrap();
        project
            .paint_tile(K, art_index, 0, 0, 1, [16, 0], 3, false)
            .unwrap();
        let mut app = app(project);
        let ctx = egui::Context::default();
        app.repaint = Some(ctx.clone());
        app.layer = other_index;
        app.restore_canvas_atlas_memory(&ctx, other_atlas);
        assert!(app.set_atlas_selection(other_atlas, vec![1]));
        app.remember_canvas_atlas_memory(&ctx, other_atlas);
        assert!(app.select_element_at([4., 4.], false, None));
        assert_eq!(app.layer, art_index);
        app.restore_canvas_atlas_memory(&ctx, atlas);
        assert_eq!(app.selected_tile_ids_for(atlas), [1, 2]);
        assert!(app.random_tiles);
        assert_eq!(app.flips, 3);
        assert_eq!(palette_popout::take_focus(&ctx, atlas), Some(false));
        app.restore_canvas_atlas_memory(&ctx, other_atlas);
        assert_eq!(app.selected_tile_ids_for(other_atlas), [1]);
        app.restore_canvas_atlas_memory(&ctx, atlas);
        assert_eq!(app.selected_tile_ids_for(atlas), [1, 2]);
    }

    #[test]
    fn tile_pick_restores_saved_brush_mode_flips_and_selects_one_stacked_cell() {
        let mut p = Project::new();
        let layer = p.add_layer("Art", "Tiles", 16).unwrap();
        let ts = p
            .add_tileset(std::path::Path::new("art.png"), 16, 64, 32)
            .unwrap();
        p.assign_tileset(layer, ts).unwrap();
        p.json["defs"]["tilesets"][0]["savedSelections"] = json!([{"ids":[1,2],"mode":"Random"}]);
        p.paint_tile(K, 0, 0, 0, 0, [0, 0], 0, false).unwrap();
        p.paint_tile_stacked(K, 0, 0, 0, 1, [16, 0], 3, false, true)
            .unwrap();
        let mut a = app(p);
        a.set_atlas_selection(ts, vec![3]);
        assert!(a.select_element_at([4., 4.], false, None));
        assert_eq!(a.selected_tile_ids_for(ts), vec![1, 2]);
        assert!(a.random_tiles);
        assert_eq!(a.flips, 3);
        let g = a.selection_group.as_ref().unwrap();
        assert_eq!(g["elementCount"], 1);
        assert_eq!(
            g["layers"][0]["clipboard"]["gridTiles"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(a.select_element_at([4., 4.], true, Some(egui::Pos2::ZERO)));
        assert_eq!(
            a.selected_tile_ids_for(ts),
            vec![1],
            "source does not recall a saved group which already intersects the brush"
        );
        if let Some(tools::Gesture::SelectionMove {
            world_end,
            moved,
            copy,
            ..
        }) = &mut a.gesture
        {
            assert!(*copy);
            *world_end = [36., 4.];
            *moved = true;
        } else {
            panic!("copy gesture missing");
        }
        a.finish_gesture();
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
    #[test]
    fn inactive_filters_and_single_layer_priority_match_source() {
        let mut p = Project::new();
        p.add_layer("Bottom", "IntGrid", 16).unwrap();
        let top = p.add_layer("Top", "IntGrid", 16).unwrap();
        p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
        p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
        let mut a = app(p);
        a.layer = 1;
        a.single_layer = true;
        assert!(a.select_element_at([4., 4.], false, None));
        assert_eq!(a.layer, 1);
        a.project
            .as_mut()
            .unwrap()
            .paint_int_grid(K, 1, 0, 0, 0)
            .unwrap();
        assert!(a.select_element_at([4., 4.], false, None));
        assert_eq!(
            a.layer, 0,
            "single-layer mode still falls back when active has no hit"
        );
        a.layer = 1;
        a.single_layer = false;
        a.project.as_mut().unwrap().json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["uid"] == top)
            .unwrap()["canSelectWhenInactive"] = json!(false);
        assert!(!a.select_element_at([4., 4.], false, None));
        let def = a.project.as_mut().unwrap().json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["uid"] == top)
            .unwrap();
        def["canSelectWhenInactive"] = json!(true);
        def["inactiveOpacity"] = json!(0.);
        assert!(!a.select_element_at([4., 4.], false, None));
        a.layer = 0;
        a.details = false;
        assert!(!a.select_element_at([4., 4.], false, None));
    }
    #[test]
    fn native_alt_pointer_press_then_drag_moves_the_picked_cell() {
        let mut p = Project::new();
        p.add_layer("Grid", "IntGrid", 16).unwrap();
        p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
        let mut a = app(p);
        a.viewport.center = [0., 0.];
        a.viewport.zoom = 1.;
        let ctx = egui::Context::default();
        let modifiers = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        let start = egui::pos2(260., 260.);
        let end = egui::pos2(292., 260.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers,
        };
        for events in [
            vec![egui::Event::PointerMoved(start)],
            vec![button(start, true)],
            vec![egui::Event::PointerMoved(end)],
            vec![button(end, false)],
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(512., 512.),
                    )),
                    events,
                    modifiers,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(512., 512.),
                                Sense::click_and_drag(),
                            );
                            a.canvas_input(ctx, rect, &response);
                        });
                },
            );
        }
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["intGridCsv"][0],
            0
        );
        assert_eq!(
            a.project.as_ref().unwrap().layer(K, 0).unwrap()["intGridCsv"][2],
            1
        );
        assert!(a.selection_group.is_some());
    }
    #[test]
    fn hollow_and_ellipse_shapes_hit_edges_instead_of_their_bounding_box() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let def = p.add_entity_def("Ring", 64, 64).unwrap();
        p.update_entity_definition(def, &json!({"pivotX":0.,"pivotY":0.}))
            .unwrap();
        p.add_entity(K, 0, def, 0, 0).unwrap();
        let d = p.json["defs"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["uid"] == def)
            .unwrap();
        d["renderMode"] = json!("Ellipse");
        d["hollow"] = json!(true);
        let entity = &p.layer(K, 0).unwrap()["entityInstances"][0];
        assert!(!entity_is_over(&p, entity, [32., 32.]));
        assert!(!entity_is_over(&p, entity, [0., 0.]));
        assert!(entity_is_over(&p, entity, [-2., 32.]));
    }
}

#[cfg(test)]
mod preview_154_sort_tests {
    use super::*;
    #[test]
    fn entity_depth_order_drives_render_order_and_hit_testing() {
        let mut p = Project::new();
        let layer_uid = p.add_layer("Actors", "Entities", 16).unwrap();
        let actor = p.add_entity_def("Actor", 64, 64).unwrap();
        p.update_entity_definition(actor, &json!({"pivotX":0.,"pivotY":0.}))
            .unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, actor, 0, 16).unwrap();
        p.add_entity(key, 0, actor, 0, 0).unwrap();
        for (sort, expected) in [("Manual", 1), ("YAscending", 0), ("YDescending", 1)] {
            p.update_layer_properties(layer_uid, &json!({"entityDepthSorting":sort}))
                .unwrap();
            let layer = p.layer(key, 0).unwrap();
            let order = canvas::entity_display_order(layer, p.layer_def(layer_uid).unwrap());
            assert_eq!(order.last().copied(), Some(expected));
            assert_eq!(hit_entity(&p, layer, [32., 32.]).unwrap().0, expected);
            assert!(
                matches!(layer_hit(&p, layer, [32., 32.], [2, 2]), Some(Element::Entity(i)) if i == expected)
            );
        }
    }
}
