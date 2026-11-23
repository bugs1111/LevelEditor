use super::*;

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
    pub(super) fn start_created_entity_transform(
        &mut self,
        key: LevelKey,
        press: egui::Pos2,
        level_pixel: [f32; 2],
        marquee_cell: Option<[i32; 2]>,
    ) {
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
        let gesture = if def["resizableX"] == true || def["resizableY"] == true {
            tools::Gesture::EntityResize {
                key,
                layer: self.layer,
                index,
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
        } else if let Some(cell) = marquee_cell {
            // Shift starts SelectionTool with its rectangle mode already active.
            if let Ok(group) = self.element_selection_group(
                project,
                key,
                self.layer,
                cell,
                selection_click::Element::Entity(index),
            ) {
                self.set_selection_group(group);
            }
            tools::Gesture::Rectangle {
                start: cell,
                end: cell,
                erase: false,
                select: true,
                entered_level: true,
                started: Instant::now(),
                world_end: level_pixel,
            }
        } else {
            tools::Gesture::EntityMove {
                key,
                layer: self.layer,
                index,
                press,
                start: level_pixel,
                end: level_pixel,
                origin,
                moved: false,
            }
        };
        self.gesture = Some(gesture);
        self.panel = Some(Panel::Entity);
    }
}
