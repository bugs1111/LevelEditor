//! Entity resize handles following the original ResizeTool's geometry and hit order.
use super::*;

const HANDLE_RADIUS: f32 = 5.0;
// Constructor order in EditorTypes.RectHandlePos determines hits when circles overlap.
const HANDLES: [[i32; 2]; 8] = [
    [0, -1],
    [0, 1],
    [-1, 0],
    [1, 0],
    [-1, -1],
    [1, -1],
    [-1, 1],
    [1, 1],
];
fn handle_active(def: &Value, handle: [i32; 2]) -> bool {
    (handle[0] == 0 || def["resizableX"] == true) && (handle[1] == 0 || def["resizableY"] == true)
}
fn entity_rect(entity: &Value, def: &Value) -> ([f32; 2], [f32; 2]) {
    let size = [number(entity, "width", 16.), number(entity, "height", 16.)];
    let top_left = std::array::from_fn(|axis| {
        let pivot = number(def, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
        (entity["px"][axis].as_f64().unwrap_or(0.) as f32 - size[axis] * pivot + 0.5).floor()
    });
    (top_left, size)
}
fn handle_position(entity: &Value, def: &Value, handle: [i32; 2]) -> [f32; 2] {
    let ([left, top], [w, h]) = entity_rect(entity, def);
    [
        match handle[0] {
            -1 => left - HANDLE_RADIUS,
            1 => left + w - 1. + HANDLE_RADIUS,
            _ => left + w * 0.5,
        },
        match handle[1] {
            -1 => top - HANDLE_RADIUS,
            1 => top + h + HANDLE_RADIUS,
            _ => top + h * 0.5,
        },
    ]
}
fn handle_at(entity: &Value, def: &Value, local: [f32; 2]) -> Option<[i32; 2]> {
    HANDLES.into_iter().find(|handle| {
        if !handle_active(def, *handle) {
            return false;
        }
        let p = handle_position(entity, def, *handle);
        (local[0] - p[0]).powi(2) + (local[1] - p[1]).powi(2) <= HANDLE_RADIUS * HANDLE_RADIUS
    })
}

pub(super) fn requested_entity_size(
    def: &Value,
    entity: &Value,
    cursor: [f32; 2],
    handle: [i32; 2],
) -> [i32; 2] {
    let (top_left, size) = entity_rect(entity, def);
    let initial = size.map(|value| value as i32);
    let mut requested = initial;
    for axis in 0..2 {
        if def[if axis == 0 { "resizableX" } else { "resizableY" }] == true {
            requested[axis] = match handle[axis] {
                -1 => (top_left[axis] + size[axis] - HANDLE_RADIUS - cursor[axis]) as i32,
                1 => (cursor[axis] - top_left[axis] - HANDLE_RADIUS) as i32,
                _ => initial[axis],
            };
        }
    }
    // Original corner handles use width to control aspect; top/bottom use height.
    if handle[0] != 0
        && def["resizableX"] == true
        && def["resizableY"] == true
        && def["keepAspectRatio"] == true
    {
        requested[1] = initial[1];
    }
    requested
}

pub(super) fn resized_entity_origin(
    def: &Value,
    origin: [i32; 2],
    initial: [i32; 2],
    result: [i32; 2],
    handle: [i32; 2],
) -> [i32; 2] {
    let mut moved = origin;
    for (axis, key) in ["pivotX", "pivotY"].into_iter().enumerate() {
        let pivot = number(def, key, 0.);
        let direction = if handle[axis] < 0 && pivot == 0. {
            -1
        } else if handle[axis] > 0 && pivot == 1. {
            1
        } else {
            0
        };
        moved[axis] = (i64::from(origin[axis])
            + i64::from(direction) * (i64::from(result[axis]) - i64::from(initial[axis])))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    }
    moved
}

impl EditorApp {
    fn single_entity_selected(&self, key: LevelKey, layer: usize, index: usize) -> bool {
        let Some(group) = &self.selection_group else {
            return true;
        };
        if group["elementCount"].as_u64() != Some(1) {
            return false;
        }
        let Some(entity) = self
            .project
            .as_ref()
            .and_then(|p| p.layer(key, layer))
            .and_then(|l| l["entityInstances"].get(index))
        else {
            return false;
        };
        group["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| {
                entry["clipboard"]["entityInstances"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["iid"] == entity["iid"]))
            })
    }
    pub(super) fn resize_handle_at(
        &self,
        key: LevelKey,
        layer: usize,
        index: usize,
        level_pixel: [f32; 2],
    ) -> Option<[i32; 2]> {
        if !self.single_entity_selected(key, layer, index) {
            return None;
        }
        let project = self.project.as_ref()?;
        let entity = project
            .layer(key, layer)?
            .get("entityInstances")?
            .get(index)?;
        let def = project.json["defs"]["entities"]
            .as_array()?
            .iter()
            .find(|d| d["uid"] == entity["defUid"])?;
        handle_at(entity, def, level_pixel)
    }
    pub(super) fn draw_resize_handles(&self, ui: &egui::Ui, rect: Rect) {
        if self.world || !self.details {
            return;
        }
        let Some(key) = self.selected else { return };
        let Some(index) = self.entity_index else {
            return;
        };
        if !self.single_entity_selected(key, self.layer, index) {
            return;
        }

        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        if layer["visible"] == false
            || layer["layerDefUid"]
                .as_i64()
                .is_some_and(|uid| self.hidden.contains(&uid))
        {
            return;
        }
        let Some(entity) = layer["entityInstances"].get(index) else {
            return;
        };
        let Some(def) = project.json["defs"]["entities"]
            .as_array()
            .and_then(|defs| defs.iter().find(|d| d["uid"] == entity["defUid"]))
        else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        for handle in HANDLES {
            if !handle_active(def, handle) {
                continue;
            }
            let position = handle_position(entity, def, handle);
            let screen = self.viewport.to_screen(
                position,
                rect.min.into(),
                rect.size().into(),
            );
            painter.circle_filled(
                screen.into(),
                HANDLE_RADIUS * 0.6 * self.viewport.zoom,
                Color32::from_rgba_unmultiplied(255, 204, 0, 191),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn requested_size_uses_the_current_rectangle_and_level_cursor() {
        let def = json!({"resizableX":true,"resizableY":true,"pivotX":0.5,"pivotY":1,"width":20,"height":10});
        let entity = json!({"px":[100,100],"width":20,"height":10});
        assert_eq!(
            requested_entity_size(&def, &entity, [123., 108.], [1, 1]),
            [28, 13]
        );
        assert_eq!(
            requested_entity_size(&def, &entity, [93., 88.], [-1, -1]),
            [12, 7]
        );
        assert_eq!(
            resized_entity_origin(&def, [100, 100], [20, 10], [28, 13], [1, 1]),
            [100, 103]
        );
        let def = json!({"resizableX":true,"resizableY":true,"pivotX":0,"pivotY":0});
        assert_eq!(
            resized_entity_origin(&def, [100, 100], [20, 10], [28, 13], [-1, -1]),
            [92, 97]
        );
    }
    #[test]
    fn side_handles_leave_the_other_axis_unchanged_and_fixed_axes_have_no_handles() {
        let def = json!({"resizableX":true,"resizableY":false,"pivotX":0.5,"pivotY":1});
        assert!(handle_active(&def, [-1, 0]));
        assert!(handle_active(&def, [1, 0]));
        assert!(!handle_active(&def, [1, 1]));
        assert!(!handle_active(&def, [0, 1]));
        let entity = json!({"px":[100,100],"width":20,"height":10});
        assert_eq!(
            requested_entity_size(&def, &entity, [121., 199.], [1, 0]),
            [26, 10]
        );
    }
    #[test]
    fn aspect_corner_uses_horizontal_control_and_vertical_side_uses_height() {
        let def = json!({"resizableX":true,"resizableY":true,"keepAspectRatio":true,"width":20,"height":10,"pivotX":0,"pivotY":0});
        let entity = json!({"px":[100,100],"width":20,"height":10});
        let request = requested_entity_size(&def, &entity, [125., 155.], [1, 1]);
        assert_eq!(
            ldtk_core::constrained_entity_size(&def, request, [20, 10], None),
            [20, 10]
        );
        let request = requested_entity_size(&def, &entity, [100., 125.], [0, 1]);
        assert_eq!(
            ldtk_core::constrained_entity_size(&def, request, [20, 10], None),
            [40, 20]
        );
    }
    #[test]
    fn source_handle_offsets_and_world_radius_are_used_for_hit_testing() {
        let def = json!({"resizableX":true,"resizableY":true,"pivotX":0.5,"pivotY":1});
        let entity = json!({"px":[32,32],"width":16,"height":16,"__pivot":[0.5,1]});
        assert_eq!(handle_position(&entity, &def, [1, 1]), [44., 37.]);
        assert_eq!(handle_at(&entity, &def, [32., 11.]), Some([0, -1]));
        assert_eq!(handle_at(&entity, &def, [32., 6.1]), Some([0, -1]));
        assert_eq!(handle_at(&entity, &def, [32., 5.9]), None);
    }
}
#[cfg(test)]
mod native_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn native_hit_and_overlay_use_the_selected_definition_and_emit_eight_handles() {
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let uid = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(
                uid,
                &json!({"resizableX":true,"resizableY":true,"pivotX":0.5,"pivotY":1}),
            )
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, uid, 32, 32).unwrap();
        app.project = Some(project);
        app.selected = Some(key);
        app.entity_index = Some(0);
        app.viewport.zoom = 2.;
        assert_eq!(
            app.resize_handle_at(key, 0, 0, [32., 11.]),
            Some([0, -1])
        );
        let ctx = egui::Context::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256., 256.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.draw_resize_handles(ui, ui.max_rect()));
            },
        );
        let circles: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Circle(circle) = &shape.shape {
                    Some(circle)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(circles.len(), 8);
        assert!(circles.iter().all(|circle| circle.radius == 6.));
    }
}
