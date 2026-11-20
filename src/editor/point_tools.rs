use super::*;
use serde_json::json;

pub(super) fn hit_point(
    project: &Project,
    layer: &Value,
    local: [f32; 2],
) -> Option<(usize, i64, usize)> {
    let grid = number(layer, "__gridSize", 16.0).max(1.0);
    let cell = local.map(|axis| (axis.trunc() / grid).floor() as i32);
    match selection_click::layer_hit(project, layer, local, cell) {
        Some(selection_click::Element::Point {
            entity,
            field,
            index,
        }) => Some((entity, field, index)),
        _ => None,
    }
}

impl EditorApp {
    pub(super) fn point_interaction(
        &mut self,
        key: LevelKey,
        local: [f32; 2],
        world: [f32; 2],
        cell: [i32; 2],
        erase: bool,
    ) -> bool {
        let Some((entity, uid, index)) = self.project.as_ref().and_then(|p| {
            p.layer(key, self.layer)
                .and_then(|l| hit_point(p, l, local))
        }) else {
            return false;
        };
        let iid = self
            .project
            .as_ref()
            .unwrap()
            .layer(key, self.layer)
            .unwrap()["entityInstances"][entity]["iid"]
            .clone();
        self.capture_selection([cell[0], cell[1], 1, 1]);
        let Some(mut group) = self.selection_group.take() else {
            return true;
        };
        for entry in group["layers"].as_array_mut().into_iter().flatten() {
            entry["clipboard"]["intGridCsv"] = json!([]);
            entry["clipboard"]["gridTiles"] = json!([]);
            entry["clipboard"]["entityInstances"] = json!([]);
            entry["cellMask"] = json!([]);
            if let Some(points) = entry["points"].as_array_mut() {
                points.retain(|p| {
                    p["entityIid"] == iid && p["fieldDefUid"] == uid && p["arrayIndex"] == index
                });
            }
        }
        group["layers"]
            .as_array_mut()
            .unwrap()
            .retain(|entry| entry["points"].as_array().is_some_and(|p| !p.is_empty()));
        group["elementCount"] = json!(1);
        group["emptySpace"] = json!(false);
        group["originalRects"] = json!([]);
        if erase {
            let owner = self
                .project
                .as_ref()
                .unwrap()
                .layer(key, self.layer)
                .unwrap()["entityInstances"][entity]
                .clone();
            let project = self.project.as_ref().unwrap();
            let definition = project.json["defs"]["entities"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["uid"] == owner["defUid"])
                .unwrap();
            let color = canvas::entity_smart_color(project, &owner, definition, true);
            let grid = number(project.layer(key, self.layer).unwrap(), "__gridSize", 16.);
            self.chain_entity = None;
            if self
                .edit_level("Point deleted", key, |p| {
                    p.delete_selection_group(key, &group)
                })
                .is_some()
            {
                self.bleep_entity(key, self.layer, &owner);
                self.bleep_point(
                    key,
                    [(cell[0] as f32 + 0.5) * grid, (cell[1] as f32 + 0.5) * grid],
                    color,
                );
            }
            self.selection = None;
            self.selection_group = None;
            self.entity_index = Some(entity);
            self.panel = Some(Panel::Entity);
        } else {
            self.history.begin(self.project.as_ref().unwrap(), key);
            self.stroke = true;
            self.last_chance = None;
            self.set_selection_group(group);
            self.gesture = Some(tools::Gesture::SelectionMove {
                region: [cell[0], cell[1], 1, 1],
                start: cell,
                end: cell,
                copy: false,
                world_start: world,
                world_end: world,
                moved: false,
            });
            self.entity_index = Some(entity);
        }
        true
    }

    pub(super) fn start_created_entity_transform(&mut self, key: LevelKey, local: [f32; 2]) {
        let Some(index) = self.entity_index else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let Some(entity) = project
            .layer(key, self.layer)
            .and_then(|l| l["entityInstances"].get(index))
        else {
            return;
        };
        let Some(def) = project.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["uid"] == entity["defUid"])
        else {
            return;
        };
        let origin = [
            entity["px"][0].as_i64().unwrap_or(0) as i32,
            entity["px"][1].as_i64().unwrap_or(0) as i32,
        ];
        self.gesture = Some(if def["resizableX"] == true || def["resizableY"] == true {
            tools::Gesture::EntityResize {
                key,
                layer: self.layer,
                index,
                start: local,
                size: [
                    number(entity, "width", 16.0) as i32,
                    number(entity, "height", 16.0) as i32,
                ],
                origin,
                handle: [
                    i32::from(def["resizableX"] == true),
                    i32::from(def["resizableY"] == true),
                ],
            }
        } else {
            tools::Gesture::EntityMove {
                key,
                layer: self.layer,
                index,
                start: local,
                end: local,
                origin,
                moved: false,
            }
        });
        self.panel = Some(Panel::Entity);
    }
}
