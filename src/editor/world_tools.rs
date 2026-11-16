use super::*;

fn bounds_distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    ((a[0] - b[2]).max(0.) + (b[0] - a[2]).max(0.))
        .max((a[1] - b[3]).max(0.) + (b[1] - a[3]).max(0.))
}
fn bounds_at(position: [i32; 2], size: [i32; 2]) -> [f32; 4] {
    [
        position[0] as f32,
        position[1] as f32,
        (position[0] + size[0]) as f32,
        (position[1] + size[1]) as f32,
    ]
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

impl EditorApp {
    pub(super) fn level_at_world(&self, position: [f32; 2]) -> Option<LevelKey> {
        let project = self.project.as_ref()?;
        let position = position.map(|value| (value + 0.5).floor());
        canvas::placements(project, self.selected?)
            .into_iter()
            .rev()
            .find(|(key, bounds)| {
                project
                    .level(*key)
                    .is_some_and(|level| number(level, "worldDepth", 0.) as i64 == self.world_depth)
                    && position[0] >= bounds[0]
                    && position[0] < bounds[2]
                    && position[1] >= bounds[1]
                    && position[1] < bounds[3]
            })
            .map(|(key, _)| key)
    }

    pub(super) fn world_drag_position(
        &self,
        key: LevelKey,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        disable_edge_snap: bool,
    ) -> [i32; 2] {
        let Some(project) = &self.project else {
            return origin;
        };
        let Some(level) = project.level(key) else {
            return origin;
        };
        let container = key
            .world
            .and_then(|world| project.json["worlds"].get(world))
            .unwrap_or(&project.json);
        let start = start.map(|value| (value + 0.5).floor() as i32);
        let end = end.map(|value| (value + 0.5).floor() as i32);
        if container["worldLayout"] == "GridVania" {
            let grid = [
                number(container, "worldGridWidth", 256.).max(1.) as i32,
                number(container, "worldGridHeight", 256.).max(1.) as i32,
            ];
            return std::array::from_fn(|axis| {
                let value = origin[axis]
                    + (end[axis].div_euclid(grid[axis]) - start[axis].div_euclid(grid[axis]))
                        * grid[axis];
                value.div_euclid(grid[axis]) * grid[axis]
            });
        }
        if text(container, "worldLayout", "Free").starts_with("Linear") {
            return origin;
        }
        let grid = project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|layer| layer["type"] != "Entities")
            .map(|layer| number(layer, "gridSize", 16.).max(1.) as i32)
            .min()
            .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.).max(1.) as i32);
        let mut position = std::array::from_fn(|axis| {
            let value = origin[axis] + end[axis] - start[axis];
            if self.grid {
                value / grid * grid
            } else {
                value
            }
        });
        if disable_edge_snap {
            return position;
        }
        let size = [
            number(level, "pxWid", 256.) as i32,
            number(level, "pxHei", 256.) as i32,
        ];
        let neighbours = canvas::placements(project, key)
            .into_iter()
            .filter(|(candidate, _)| *candidate != key)
            .map(|(_, bounds)| bounds)
            .collect::<Vec<_>>();
        let distance = grid as f32 / (self.viewport.zoom * 0.4);
        for neighbour in &neighbours {
            if bounds_distance(bounds_at(position, size), *neighbour) > distance {
                continue;
            }
            // Source snaps X, Y, then X again: Y can make an X candidate valid.
            for axis in [0, 1, 0] {
                for offset in [0, size[axis]] {
                    for edge in [neighbour[axis], neighbour[axis + 2]] {
                        if (position[axis] as f32 + offset as f32 - edge).abs() > distance {
                            continue;
                        }
                        let mut candidate = position;
                        candidate[axis] = edge as i32 - offset;
                        if !neighbours
                            .iter()
                            .any(|other| overlaps(bounds_at(candidate, size), *other))
                        {
                            position = candidate;
                        }
                    }
                }
            }
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn editor() -> EditorApp {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.world = true;
        app.viewport.zoom = 1.;
        app
    }
    #[test]
    fn world_hit_only_selects_the_active_depth_and_prefers_last_same_depth_level() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Overlap", 256, 256).unwrap();
        p.move_level(second, 0, 0, 1).unwrap();
        assert_eq!(app.level_at_world([30., 30.]), Some(first));
        app.world_depth = 1;
        assert_eq!(app.level_at_world([30., 30.]), Some(second));
        app.world_depth = 2;
        assert_eq!(app.level_at_world([30., 30.]), None);
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 0, 0, 0)
            .unwrap();
        app.world_depth = 0;
        assert_eq!(app.level_at_world([30., 30.]), Some(second));
    }
    #[test]
    fn free_drag_uses_smallest_nonentity_grid_and_truncates_negative_coordinates() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        p.add_layer("Objects", "Entities", 4).unwrap();
        p.add_layer("Ground", "IntGrid", 16).unwrap();
        p.add_layer("Fine", "Tiles", 8).unwrap();
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [19., -19.], [0, 0], true),
            [16, -16]
        );
        app.grid = false;
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [19., -19.], [0, 0], true),
            [19, -19]
        );
    }
    #[test]
    fn free_drag_snaps_near_edges_unless_modifiers_disable_and_does_not_snap_into_overlap() {
        let mut app = editor();
        app.grid = false;
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let other = p.add_level(None, "Neighbour", 256, 256).unwrap();
        p.move_level(other, 300, 0, 0).unwrap();
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], false),
            [44, 0]
        );
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], true),
            [40, 0]
        );
        let p = app.project.as_mut().unwrap();
        let blocker = p.add_level(None, "Blocker", 16, 256).unwrap();
        p.move_level(blocker, 290, 0, 3).unwrap(); // Other depths also block snapping in the source.
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], false),
            [34, 0]
        );
    }
    #[test]
    fn gridvania_drag_uses_origin_cell_delta_and_floor_for_negative_world_positions() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(256);
        p.json["worldGridHeight"] = json!(128);
        assert_eq!(
            app.world_drag_position(key, [10., 10.], [-1., 140.], [0, 0], false),
            [-256, 128]
        );
    }
}
