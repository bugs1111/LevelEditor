use super::*;
use serde_json::json;

pub(super) fn selection_drag_started(ctx: &egui::Context, press: egui::Pos2) -> bool {
    let page = |point: egui::Pos2| egui::pos2(point.x.trunc(), point.y.trunc());
    let press = page(press);
    ctx.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    ..
                } => break,
                egui::Event::PointerMoved(point) if press.distance(page(*point)) >= 10. => {
                    return true;
                }
                _ => {}
            }
        }
        false
    })
}

impl EditorApp {
    pub(super) fn selection_geometries(&self, key: LevelKey) -> Value {
        let Some(project) = &self.project else {
            return json!([]);
        };
        json!(project.level(key).and_then(|l|l["layerInstances"].as_array()).into_iter().flatten().enumerate().map(|(index,layer)|{
            let origin=canvas::layer_to_world(project,key,index,&self.viewport,[0.0,0.0]);let unit=canvas::layer_to_world(project,key,index,&self.viewport,[1.0,0.0]);
            let visible=self.canvas_layer_visible(key,index)&&(!self.single_layer||index==self.layer);
            json!({"layerDefUid":layer["layerDefUid"],"origin":origin,"scale":unit[0]-origin[0],"visible":visible})
        }).collect::<Vec<_>>())
    }

    fn selection_smart_layer(&self) -> Option<(LevelKey, usize, f32, f32)> {
        let key = self.selected?;
        let project = self.project.as_ref()?;
        let group = self.selection_group.as_ref()?;
        group["layers"]
            .as_array()?
            .iter()
            .filter(|entry| {
                ["entityInstances", "gridTiles"].iter().any(|name| {
                    entry["clipboard"][*name]
                        .as_array()
                        .is_some_and(|v| !v.is_empty())
                }) || entry["points"].as_array().is_some_and(|v| !v.is_empty())
                    || entry["clipboard"]["intGridCsv"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|v| v.as_i64().is_some_and(|v| v != 0)))
            })
            .filter_map(|entry| {
                let index = project.level(key)?["layerInstances"]
                    .as_array()?
                    .iter()
                    .position(|l| l["layerDefUid"] == entry["layerDefUid"])?;
                let grid = number(project.layer(key, index)?, "__gridSize", 16.0);
                let a = canvas::layer_to_world(project, key, index, &self.viewport, [0.0; 2]);
                let b = canvas::layer_to_world(project, key, index, &self.viewport, [1.0, 0.0]);
                Some((key, index, grid, b[0] - a[0]))
            })
            .reduce(|best, item| {
                if item.2 * item.3 > best.2 * best.3 {
                    item
                } else {
                    best
                }
            })
    }
    pub(super) fn selection_world_delta(&self, start: [f32; 2], end: [f32; 2]) -> [f64; 2] {
        if !self.grid
            && self
                .selection_group
                .as_ref()
                .is_some_and(ldtk_core::selection_group_is_entity_only)
        {
            return std::array::from_fn(|axis| f64::from((end[axis] - start[axis]).trunc()));
        }
        let Some((key, index, grid, scale)) = self.selection_smart_layer() else {
            return [0.0; 2];
        };
        let project = self.project.as_ref().unwrap();
        let a = canvas::world_to_layer(project, key, index, &self.viewport, start);
        let b = canvas::world_to_layer(project, key, index, &self.viewport, end);
        std::array::from_fn(|axis| {
            f64::from(((b[axis] / grid).floor() - (a[axis] / grid).floor()) * grid * scale)
        })
    }
    pub(super) fn selection_arrow_origin(&self, point: [f32; 2]) -> [f32; 2] {
        if !self.grid
            && self
                .selection_group
                .as_ref()
                .is_some_and(ldtk_core::selection_group_is_entity_only)
        {
            return point;
        }
        let Some((key, index, grid, _)) = self.selection_smart_layer() else {
            return point;
        };
        let project = self.project.as_ref().unwrap();
        let p = canvas::world_to_layer(project, key, index, &self.viewport, point);
        canvas::layer_to_world(
            project,
            key,
            index,
            &self.viewport,
            p.map(|v| ((v / grid).floor() + 0.5) * grid),
        )
    }
    pub(super) fn selection_contains_world(&self, world: [f32; 2]) -> bool {
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return false;
        };
        let Some(group) = &self.selection_group else {
            return self.selection.is_some_and(|r| {
                let grid = project
                    .layer(key, self.layer)
                    .map(|l| number(l, "__gridSize", 16.0))
                    .unwrap_or(16.0);
                let p = canvas::world_to_layer(project, key, self.layer, &self.viewport, world);
                p[0] >= r[0] as f32 * grid
                    && p[1] >= r[1] as f32 * grid
                    && p[0] < (r[0] + r[2]) as f32 * grid
                    && p[1] < (r[1] + r[3]) as f32 * grid
            });
        };
        if group["emptySpace"] == true
            && group["originalRects"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| {
                    world[0] >= number(r, "leftPx", 0.0)
                        && world[0] <= number(r, "rightPx", 0.0)
                        && world[1] >= number(r, "topPx", 0.0)
                        && world[1] <= number(r, "bottomPx", 0.0)
                })
        {
            return true;
        }
        group["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| {
                let Some(index) = project
                    .level(key)
                    .and_then(|l| l["layerInstances"].as_array())
                    .and_then(|layers| {
                        layers
                            .iter()
                            .position(|l| l["layerDefUid"] == entry["layerDefUid"])
                    })
                else {
                    return false;
                };
                let layer = project.layer(key, index).unwrap();
                let grid = number(layer, "__gridSize", 16.0);
                let p = canvas::world_to_layer(project, key, index, &self.viewport, world);
                let cell = p.map(|v| (v / grid).floor() as i64);
                let left = entry["rect"][0].as_i64().unwrap_or(0);
                let top = entry["rect"][1].as_i64().unwrap_or(0);
                let width = entry["clipboard"]["width"].as_i64().unwrap_or(0);
                let i = (cell[1] - top) * width + cell[0] - left;
                if cell[0] >= left
                    && cell[0] < left + width
                    && entry["cellMask"]
                        .as_array()
                        .is_some_and(|mask| mask.iter().any(|v| v.as_i64() == Some(i)))
                {
                    return true;
                }
                if entry["clipboard"]["gridTiles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|t| {
                        (t["px"][0].as_f64().unwrap_or(0.0) as f32 / grid).floor() as i64 + left
                            == cell[0]
                            && (t["px"][1].as_f64().unwrap_or(0.0) as f32 / grid).floor() as i64
                                + top
                                == cell[1]
                    })
                {
                    return true;
                }
                if entry["points"].as_array().into_iter().flatten().any(|pt| {
                    pt["value"]["cx"].as_i64() == Some(cell[0])
                        && pt["value"]["cy"].as_i64() == Some(cell[1])
                }) {
                    return true;
                }
                entry["clipboard"]["entityInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|selected| {
                        layer["entityInstances"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|e| e["iid"] == selected["iid"])
                            .is_some_and(|e| selection_click::entity_is_over(project, e, p))
                    })
            })
    }
    pub(super) fn set_selection_group(&mut self, group: Value) {
        if group["elementCount"].as_u64().is_some_and(|n| n > 1) {
            self.entity_index = None;
            if self.panel == Some(Panel::Entity) {
                self.panel = None;
            }
        }
        if group["elementCount"].as_u64() == Some(0) {
            self.selection = None;
            self.selection_group = None;
            return;
        }
        if let Some(layer) = self
            .selected
            .and_then(|k| self.project.as_ref()?.layer(k, self.layer))
        {
            let grid = number(layer, "__gridSize", 16.0);
            if group["geometryAware"] == true {
                let project = self.project.as_ref().unwrap();
                let key = self.selected.unwrap();
                let origin = [
                    group["origin"][0].as_f64().unwrap_or(0.0) as f32,
                    group["origin"][1].as_f64().unwrap_or(0.0) as f32,
                ];
                let a = canvas::world_to_layer(project, key, self.layer, &self.viewport, origin);
                let b = canvas::world_to_layer(
                    project,
                    key,
                    self.layer,
                    &self.viewport,
                    [
                        origin[0] + group["size"][0].as_f64().unwrap_or(0.0) as f32,
                        origin[1] + group["size"][1].as_f64().unwrap_or(0.0) as f32,
                    ],
                );
                self.selection = Some([
                    (a[0] / grid).floor() as i32,
                    (a[1] / grid).floor() as i32,
                    ((b[0] - a[0]) / grid).ceil() as i32,
                    ((b[1] - a[1]) / grid).ceil() as i32,
                ]);
                self.selection_group = Some(group);
                return;
            }
            self.selection = Some([
                ((group["origin"][0].as_f64().unwrap_or(0.0) as f32
                    - number(layer, "__pxTotalOffsetX", 0.0))
                    / grid)
                    .floor() as i32,
                ((group["origin"][1].as_f64().unwrap_or(0.0) as f32
                    - number(layer, "__pxTotalOffsetY", 0.0))
                    / grid)
                    .floor() as i32,
                (group["size"][0].as_f64().unwrap_or(0.0) as f32 / grid).ceil() as i32,
                (group["size"][1].as_f64().unwrap_or(0.0) as f32 / grid).ceil() as i32,
            ]);
        }
        self.selection_group = Some(group);
    }
    fn selectable_layers(&self) -> Vec<usize> {
        let Some(project) = &self.project else {
            return vec![];
        };
        let Some(level) = self.selected.and_then(|k| project.level(k)) else {
            return vec![];
        };
        level["layerInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(i, l)| {
                let d = l["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid));
                (*i == self.layer || (!self.single_layer && d.is_some_and(|d| d["canSelectWhenInactive"] != false)))
                    && l["__type"] != "AutoLayer"
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn capture_selection(&mut self, region: [i32; 4]) {
        self.capture_selection_rectangle(region, false);
    }

    pub(super) fn capture_selection_rectangle(&mut self, region: [i32; 4], append: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let grid = number(layer, "__gridSize", 16.0);
        let a = canvas::layer_to_pointer_world(
            project,
            key,
            self.layer,
            &self.viewport,
            [region[0] as f32 * grid, region[1] as f32 * grid],
        );
        let b = canvas::layer_to_pointer_world(
            project,
            key,
            self.layer,
            &self.viewport,
            [
                (region[0] + region[2]) as f32 * grid,
                (region[1] + region[3]) as f32 * grid,
            ],
        );
        let a = a.map(f32::trunc);
        let b = b.map(|v| (v - 1.).trunc());
        let bounds = [
            a[0] as i32,
            a[1] as i32,
            (b[0] - a[0]) as i32 + 1,
            (b[1] - a[1]) as i32 + 1,
        ];
        let geometries = self.selection_geometries(key);
        let regions = json!(
            self.selectable_layers()
                .iter()
                .filter(|&&index| geometries[index]["visible"] == true)
                .map(|&index| {
                    let layer = project.layer(key, index).unwrap();
                    let grid = number(layer, "__gridSize", 16.0);
                    let left = canvas::pointer_to_layer(project, key, index, &self.viewport, a);
                    let right = canvas::pointer_to_layer(project, key, index, &self.viewport, b);
                    let x = (left[0] / grid) as i32;
                    let y = (left[1] / grid) as i32;
                    let w = (right[0] / grid) as i32 - x + 1;
                    let h = (right[1] / grid) as i32 - y + 1;
                    json!({"layerIndex":index,"rect":[x,y,w,h],"geometry":geometries[index]})
                })
                .collect::<Vec<_>>()
        );
        let result = project.copy_selection_group_from_regions(key, &regions, bounds)
            .and_then(|mut group| {
                group["emptySpace"] = self.settings.bool("emptySpaceSelection").into();
                if group["emptySpace"] == false {
                    group["originalRects"] = json!([]);
                }
                if append
                    && let Some(previous) = self.selection_group.as_ref()
                    && previous["elementCount"].as_u64().is_some_and(|n| n > 0)
                {
                    if group["elementCount"] == 0 {
                        if group["emptySpace"] == false {
                            return Ok(previous.clone());
                        }
                        let level = project.level(key).unwrap();
                        group["originalRects"] = json!([{
                            "leftPx": bounds[0].max(0),
                            "rightPx": (bounds[0] + bounds[2] - 1).min(number(level, "pxWid", 0.) as i32),
                            "topPx": bounds[1].max(0),
                            "bottomPx": (bounds[1] + bounds[3] - 1).min(number(level, "pxHei", 0.) as i32)
                        }]);
                    }
                    return project.append_selection_group(key, previous, group, &geometries);
                }
                Ok(group)
            });
        match result {
            Ok(group) => {
                if append {
                    self.apply_single_marquee_pick(key, &group);
                    self.set_selection_group(group);
                    return;
                }
                if group["elementCount"].as_u64().is_some_and(|n| n > 1) {
                    self.entity_index = None;
                    if self.panel == Some(Panel::Entity) {
                        self.panel = None;
                    }
                }
                self.selection_group = Some(group);
                self.selection = Some(region);
            }
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
            }
        }
    }

    pub(super) fn select_all(&mut self) {
        if self.world {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let level = project.level(key).unwrap();
        let width = number(level, "pxWid", 1.0) as i32;
        let height = number(level, "pxHei", 1.0) as i32;
        let geometries = self.selection_geometries(key);
        let regions=json!(self.selectable_layers().into_iter().map(|index|{
            let layer=project.layer(key,index).unwrap();
            json!({"layerIndex":index,"rect":[0,0,layer["__cWid"],layer["__cHei"]],"geometry":geometries[index],"all":true})
        }).collect::<Vec<_>>());
        match project.copy_selection_group_from_regions(key, &regions, [0, 0, width, height]) {
            Ok(mut group) => {
                if self.selection_group.is_some() && self.panel == Some(Panel::Entity) {
                    self.panel = None;
                    self.entity_index = None;
                }
                let any = group["elementCount"].as_u64().is_some_and(|count| count > 0);
                group["emptySpace"] = self.settings.bool("emptySpaceSelection").into();
                group["originalRects"] = if any && group["emptySpace"] == true {
                    json!([{"leftPx":0,"topPx":0,"rightPx":width,"bottomPx":height}])
                } else {
                    json!([])
                };
                self.set_selection_group(group);
                if any {
                    let label = if self.single_layer {
                        "Selected all in layer"
                    } else {
                        "Selected all"
                    };
                    let message = self
                        .repaint
                        .as_ref()
                        .map_or(label, |ctx| locale::tr_ctx(ctx, label));
                    self.notify_quick(message.into());
                } else {
                    self.report_empty_selection();
                }
            }
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
            }
        }
    }

    pub(super) fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) {
        let Some(key) = self.selected else {
            return;
        };
        if self.selection_group.is_none()
            && let Some(region) = self.selection
        {
            self.capture_selection(region);
        }
        let Some(group) = self.selection_group.clone() else {
            return;
        };
        let geometries = self.selection_geometries(key);
        let result = if cut {
            self.edit_level_marked("Selection cut", key, |p, history| {
                let value = p.refresh_selection_group_geometry(key, &group, &geometries)?;
                bleep::mark_group(history, p, key, &value, true);
                p.delete_selection_group(key, &value)?;
                Ok(value)
            })
        } else {
            self.project.as_ref().and_then(|p| {
                p.refresh_selection_group_geometry(key, &group, &geometries)
                    .map_err(|e| {
                        self.status = e.to_string();
                    })
                    .ok()
            })
        };
        if let Some(value) = result {
            ctx.copy_text(value.to_string());
            self.clipboard = Some(value);
            if cut {
                self.selection = None;
                self.selection_group = None;
            }
        }
    }

    pub(super) fn paste_at(&mut self, cell: [i32; 2]) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(value) = self.stamp.clone().or_else(|| self.clipboard.clone()) else {
            return;
        };
        let index = self.layer;
        let flips = self.flips;
        let stack = self.tile_stacking;
        let Some(layer) = self.project.as_ref().and_then(|p| p.layer(key, index)) else {
            return;
        };
        let grid = number(layer, "__gridSize", 16.0) as i32;
        let project = self.project.as_ref().unwrap();
        let origin = canvas::layer_to_world(
            project,
            key,
            index,
            &self.viewport,
            [cell[0] as f32 * grid as f32, cell[1] as f32 * grid as f32],
        )
        .map(f64::from);
        let geometries = self.selection_geometries(key);
        let result = self.edit_level_marked("Selection pasted", key, |p, history| {
            if value["type"] == "SelectionGroup" {
                p.paste_selection_group_with_geometry(
                    key,
                    &value,
                    origin,
                    &geometries,
                    false,
                    false,
                    stack,
                )
                .map(|group| {
                    bleep::mark_group(history, p, key, &group, true);
                    Some(group)
                })
            } else {
                p.paste_region(
                    key,
                    index,
                    &value,
                    cell[0],
                    cell[1],
                    flips & 1 != 0,
                    flips & 2 != 0,
                    stack,
                )?;
                p.rebuild_auto_layers(key)?;
                Ok(None)
            }
        });
        if let Some(Some(group)) = result {
            self.set_selection_group(group);
        }
    }

    pub(super) fn delete_selection(&mut self) {
        let Some(key) = self.selected else {
            return;
        };
        if self.selection_group.is_none()
            && let Some(region) = self.selection
        {
            self.capture_selection(region);
        }
        let removed = if let Some(group) = self.selection_group.clone() {
            self.edit_level_marked("Selection deleted", key, |p, history| {
                bleep::mark_group(history, p, key, &group, true);
                p.delete_selection_group(key, &group)
            })
            .is_some()
        } else if let Some(index) = self.entity_index {
            let layer = self.layer;
            self.edit_level_marked("Entity deleted", key, |p, history| {
                if let Some(entity) = p
                    .layer(key, layer)
                    .and_then(|l| l["entityInstances"].get(index))
                {
                    history.mark_entity_change(p, key, layer, entity);
                }
                p.remove_entity_with_references(key, layer, index)
            }) == Some(true)
        } else {
            false
        };
        if removed {
            self.selection = None;
            self.selection_group = None;
            self.entity_index = None;
            if self.panel == Some(Panel::Entity) {
                self.panel = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_all_includes_hidden_layers_except_inactive_protected_layers() {
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        let actor_layer = project.add_layer("Actors", "Entities", 16).unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        project.add_entity(key, 0, actor, 24, 24).unwrap();
        project.paint_int_grid(key, 1, 1, 1, 1).unwrap();
        project.layer_mut(key, 0).unwrap()["visible"] = false.into();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.layer = 1;

        app.select_all();
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);
        assert_eq!(app.quick_notification.as_ref().unwrap().0, "Selected all");

        app.details = false;
        app.select_all();
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);

        app.single_layer = true;
        app.select_all();
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 1);
        assert_eq!(
            app.quick_notification.as_ref().unwrap().0,
            "Selected all in layer"
        );
        app.single_layer = false;

        let project = app.project.as_mut().unwrap();
        let definition = project.json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|definition| definition["uid"] == actor_layer)
            .unwrap();
        definition["canSelectWhenInactive"] = false.into();
        app.select_all();
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 1);
    }

    #[test]
    fn select_all_reports_an_empty_level_without_creating_an_empty_space_selection() {
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.settings.value["emptySpaceSelection"] = true.into();

        app.world = true;
        app.select_all();
        assert!(app.canvas_notices.is_empty());
        app.world = false;

        app.select_all();
        assert!(app.selection.is_none());
        assert!(app.selection_group.is_none());
        assert!(matches!(
            app.canvas_notices.last().map(|entry| &entry.notice),
            Some(notifications::CanvasNotice::NothingToSelect)
        ));
    }

    #[test]
    fn select_all_uses_full_level_extent_and_respects_empty_space_selection() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let uid = project.add_entity_def("Actor", 16, 16).unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, uid, 32, 32).unwrap();
        let level = project.level(key).unwrap();
        let expected = json!([{
            "leftPx":0,"topPx":0,"rightPx":level["pxWid"],"bottomPx":level["pxHei"]
        }]);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.settings.value["emptySpaceSelection"] = true.into();
        app.select_all();
        assert_eq!(app.selection_group.as_ref().unwrap()["originalRects"], expected);

        app.settings.value["emptySpaceSelection"] = false.into();
        app.select_all();
        let group = app.selection_group.as_ref().unwrap();
        assert_eq!(group["originalRects"], json!([]));
        assert_eq!(group["elementCount"], 1);
        app.capture_selection([0, 0, 6, 6]);
        assert_eq!(app.selection_group.as_ref().unwrap()["originalRects"], json!([]));
    }

    #[test]
    fn preview_154_entity_only_selection_moves_freely_when_grid_is_hidden() {
        let mut p = Project::new();
        p.add_layer("Actors12", "Entities", 12).unwrap();
        p.add_layer("Actors16", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, def, 32, 32).unwrap();
        p.add_entity(key, 1, def, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.grid = false;
        app.capture_selection([0, 0, 6, 6]);
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);
        assert_eq!(app.selection_world_delta([32., 32.], [51., 35.]), [19., 3.]);
        app.gesture = Some(tools::Gesture::SelectionMove {
            region: [0, 0, 6, 6],
            press: egui::Pos2::ZERO,
            start: [0, 0],
            end: [0, 0],
            copy: false,
            world_start: [32., 32.],
            world_end: [51., 35.],
            moved: true,
        });
        app.finish_gesture();
        for layer in 0..2 {
            assert_eq!(
                app.project.as_ref().unwrap().layer(key, layer).unwrap()["entityInstances"][0]["px"],
                json!([51, 35])
            );
        }
        app.undo_action(false);
        for layer in 0..2 {
            assert_eq!(
                app.project.as_ref().unwrap().layer(key, layer).unwrap()["entityInstances"][0]["px"],
                json!([32, 32])
            );
        }
    }

    #[test]
    fn preview_154_mixed_entity_and_grid_selection_still_snaps_when_grid_is_hidden() {
        let mut p = Project::new();
        let grid = p.add_layer("Ground", "IntGrid", 16).unwrap();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, def, 32, 32).unwrap();
        let grid_index = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"] == grid)
            .unwrap();
        p.paint_int_grid(key, grid_index, 2, 2, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.grid = false;
        app.capture_selection([0, 0, 6, 6]);
        assert!(!ldtk_core::selection_group_is_entity_only(
            app.selection_group.as_ref().unwrap()
        ));
        assert_eq!(app.selection_world_delta([32., 32.], [51., 35.]), [16., 0.]);
    }

    #[test]
    fn empty_larger_grid_layer_never_changes_selection_movement_grid() {
        let mut p = Project::new();
        p.add_layer("EmptyLarge", "IntGrid", 64).unwrap();
        let fine = p.add_layer("Fine", "IntGrid", 16).unwrap();
        let key = p.level_keys()[0];
        let index = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"] == fine)
            .unwrap();
        p.paint_int_grid(key, index, 0, 0, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.layer = index;
        app.capture_selection([0, 0, 4, 1]);
        assert_eq!(
            app.selection_world_delta([1.0, 1.0], [17.0, 1.0]),
            [16.0, 0.0]
        );
    }
}
