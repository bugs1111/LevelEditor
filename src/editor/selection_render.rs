//! GenericLevelElementGroup selection fill and its two four-neighbour PixelOutline passes.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mark {
    Rect(Rect, [f32; 4]),
    Circle(egui::Pos2, f32, [f32; 4]),
}
const YELLOW: [f32; 4] = [1., 0.8, 0., 1.];
const BLUE: [f32; 4] = [138. / 255., 183. / 255., 1., 1.];

fn group_marks(project: &Project, key: LevelKey, view: &Viewport, group: &Value) -> Vec<Mark> {
    let mut marks = Vec::new();
    for r in group["originalRects"].as_array().into_iter().flatten() {
        marks.push(Mark::Rect(
            Rect::from_min_max(
                egui::pos2(number(r, "leftPx", 0.), number(r, "topPx", 0.)),
                egui::pos2(number(r, "rightPx", 0.), number(r, "bottomPx", 0.)),
            ),
            BLUE,
        ));
    }
    let Some(layers) = project
        .level(key)
        .and_then(|l| l["layerInstances"].as_array())
    else {
        return marks;
    };
    for entry in group["layers"].as_array().into_iter().flatten() {
        let Some((index, layer)) = layers
            .iter()
            .enumerate()
            .find(|(_, l)| l["layerDefUid"] == entry["layerDefUid"])
        else {
            continue;
        };
        let grid = number(layer, "__gridSize", 16.);
        let at = |local| -> egui::Pos2 {
            canvas::layer_to_world(project, key, index, view, local).into()
        };
        let scale = (at([1., 0.]).x - at([0., 0.]).x).abs();
        let left = entry["rect"][0].as_i64().unwrap_or(0);
        let top = entry["rect"][1].as_i64().unwrap_or(0);
        let width = entry["clipboard"]["width"].as_i64().unwrap_or(0);
        let mut cells = std::collections::BTreeSet::new();
        if width > 0 {
            for value in entry["cellMask"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                cells.insert([left + value % width, top + value / width]);
            }
        }
        for tile in entry["clipboard"]["gridTiles"]
            .as_array()
            .into_iter()
            .flatten()
        {
            cells.insert([
                left + (tile["px"][0].as_f64().unwrap_or(0.) / f64::from(grid)).floor() as i64,
                top + (tile["px"][1].as_f64().unwrap_or(0.) / f64::from(grid)).floor() as i64,
            ]);
        }
        let columns = number(layer, "__cWid", 0.) as i64;
        let occupied_tiles = layer["gridTiles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|tile| {
                [
                    (tile["px"][0].as_f64().unwrap_or(0.) / f64::from(grid)).floor() as i64,
                    (tile["px"][1].as_f64().unwrap_or(0.) / f64::from(grid)).floor() as i64,
                ]
            })
            .collect::<HashSet<_>>();
        let live_entities = layer["entityInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entity| entity["iid"].as_str().map(|iid| (iid, entity)))
            .collect::<std::collections::HashMap<_, _>>();
        for [x, y] in cells {
            let has_value = x >= 0
                && y >= 0
                && x < columns
                && layer["intGridCsv"]
                    .get((y * columns + x) as usize)
                    .is_some_and(|v| v.as_i64().is_some_and(|v| v != 0))
                || occupied_tiles.contains(&[x, y]);
            let color = if has_value {
                YELLOW
            } else {
                [BLUE[0], BLUE[1], BLUE[2], 0.6]
            };
            marks.push(Mark::Rect(
                Rect::from_two_pos(
                    at([x as f32 * grid, y as f32 * grid]),
                    at([(x + 1) as f32 * grid, (y + 1) as f32 * grid]),
                ),
                color,
            ));
        }
        for selected in entry["clipboard"]["entityInstances"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(entity) = selected["iid"]
                .as_str()
                .and_then(|iid| live_entities.get(iid))
                .copied()
            else {
                continue;
            };
            let w = number(entity, "width", 16.);
            let h = number(entity, "height", 16.);
            let x = entity["px"][0].as_f64().unwrap_or(0.) as f32
                - entity["__pivot"][0].as_f64().unwrap_or(0.) as f32 * w;
            let y = entity["px"][1].as_f64().unwrap_or(0.) as f32
                - entity["__pivot"][1].as_f64().unwrap_or(0.) as f32 * h;
            marks.push(Mark::Rect(
                Rect::from_two_pos(at([x, y]), at([x + w, y + h])),
                YELLOW,
            ));
        }
        for point in entry["points"].as_array().into_iter().flatten() {
            let value = point["entityIid"]
                .as_str()
                .and_then(|iid| live_entities.get(iid))
                .copied()
                .and_then(|e| e["fieldInstances"].as_array())
                .into_iter()
                .flatten()
                .find(|f| f["defUid"] == point["fieldDefUid"])
                .map(|f| {
                    if point["isArray"] == true {
                        &f["__value"][point["arrayIndex"].as_u64().unwrap_or(0) as usize]
                    } else {
                        &f["__value"]
                    }
                });
            if let Some(value) = value.filter(|v| !v.is_null()) {
                marks.push(Mark::Circle(
                    at([
                        (number(value, "cx", 0.) + 0.5) * grid,
                        (number(value, "cy", 0.) + 0.5) * grid,
                    ]),
                    grid * scale * 0.4,
                    YELLOW,
                ));
            }
        }
    }
    marks
}

// Port of dn.heaps.filter.PixelOutline's four cardinal samples. This is not
// Heaps Outline2D (the unrelated radial filter used for field labels).
// https://github.com/deepnight/deepnightLibs/blob/master/src/dn/heaps/filter/PixelOutline.hx
fn pixel_outline(
    input: &[[u8; 4]],
    size: [usize; 2],
    color: [f32; 3],
    knockout: f32,
) -> Vec<[u8; 4]> {
    let [width, height] = size;
    input
        .iter()
        .enumerate()
        .map(|(index, current)| {
            let x = index % width;
            let y = index / width;
            let mut neighbour = 0_f32;
            for (valid, other) in [
                (x > 0, index.wrapping_sub(1)),
                (x + 1 < width, index + 1),
                (y > 0, index.wrapping_sub(width)),
                (y + 1 < height, index + width),
            ] {
                if valid {
                    neighbour = neighbour.max(f32::from(input[other][3]) / 255.);
                }
            }
            let current = current.map(|v| f32::from(v) / 255.);
            let edge = (1. - current[3]) * neighbour;
            let alpha = edge.max(knockout.min(current[3]));
            [
                ((1. - edge) * current[0] + edge * color[0]) * alpha,
                ((1. - edge) * current[1] + edge * color[1]) * alpha,
                ((1. - edge) * current[2] + edge * color[2]) * alpha,
                alpha,
            ]
            .map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
        })
        .collect()
}

fn bounds(mark: Mark) -> Rect {
    match mark {
        Mark::Rect(r, _) => r,
        Mark::Circle(p, r, _) => Rect::from_center_size(p, Vec2::splat(r * 2.)),
    }
}
fn raster(marks: &[Mark], rect: Rect, size: [usize; 2]) -> egui::ColorImage {
    let mut pixels = vec![[0_u8; 4]; size[0] * size[1]];
    for &mark in marks {
        let b = bounds(mark).intersect(rect);
        if !b.is_positive() {
            continue;
        }
        let color = match mark {
            Mark::Rect(_, c) | Mark::Circle(_, _, c) => c,
        };
        let min = (b.min - rect.min).floor();
        let max = (b.max - rect.min).ceil();
        for y in min.y.max(0.) as usize..(max.y as usize).min(size[1]) {
            for x in min.x.max(0.) as usize..(max.x as usize).min(size[0]) {
                let p = rect.min + egui::vec2(x as f32 + 0.5, y as f32 + 0.5);
                let inside = match mark {
                    Mark::Rect(r, _) => r.contains(p),
                    Mark::Circle(c, r, _) => c.distance_sq(p) <= r * r,
                };
                if inside {
                    let current = &mut pixels[y * size[0] + x];
                    for axis in 0..3 {
                        current[axis] = ((color[axis] * color[3]
                            + f32::from(current[axis]) / 255. * (1. - color[3]))
                            * 255.)
                            .round() as u8;
                    }
                    current[3] = ((color[3] + f32::from(current[3]) / 255. * (1. - color[3]))
                        * 255.)
                        .round() as u8;
                }
            }
        }
    }
    let first = pixel_outline(&pixels, size, [1., 0.8, 0.], 0.66);
    drop(pixels);
    let second = pixel_outline(&first, size, [0.; 3], 1.);
    drop(first);
    egui::ColorImage::new(
        size,
        second
            .into_iter()
            .map(|p| {
                let b = p;
                Color32::from_rgba_premultiplied(b[0], b[1], b[2], b[3])
            })
            .collect(),
    )
}
#[derive(Clone)]
struct Cache {
    key: u64,
    texture: egui::TextureHandle,
}
fn paint_marks(ui: &egui::Ui, canvas: Rect, view: &Viewport, marks: &[Mark]) {
    use std::hash::{Hash, Hasher};
    let ppp = ui.ctx().pixels_per_point();
    let point = |p: egui::Pos2| -> egui::Pos2 {
        let p: egui::Pos2 = view
            .to_screen(p.into(), canvas.min.into(), canvas.size().into())
            .into();
        egui::pos2(p.x * ppp, p.y * ppp)
    };
    let marks = marks
        .iter()
        .map(|m| match *m {
            Mark::Rect(r, c) => Mark::Rect(Rect::from_two_pos(point(r.min), point(r.max)), c),
            Mark::Circle(p, r, c) => Mark::Circle(point(p), r * view.zoom * ppp, c),
        })
        .collect::<Vec<_>>();
    let clip = Rect::from_min_max(canvas.min * ppp, canvas.max * ppp).expand(2.);
    let rect = marks
        .iter()
        .fold(Rect::NOTHING, |r, m| r.union(bounds(*m)))
        .expand(2.)
        .intersect(clip);
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let rect = Rect::from_min_max(rect.min.floor(), rect.max.ceil());
    let size = [rect.width() as usize, rect.height() as usize];
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for n in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
        n.to_bits().hash(&mut hasher);
    }
    for m in &marks {
        match *m {
            Mark::Rect(r, c) => {
                0.hash(&mut hasher);
                for n in [r.min.x, r.min.y, r.max.x, r.max.y].into_iter().chain(c) {
                    n.to_bits().hash(&mut hasher);
                }
            }
            Mark::Circle(p, r, c) => {
                1.hash(&mut hasher);
                for n in [p.x, p.y, r].into_iter().chain(c) {
                    n.to_bits().hash(&mut hasher);
                }
            }
        }
    }
    let key = hasher.finish();
    let id = egui::Id::new("source-selection-highlight");
    let cached = ui.ctx().data_mut(|d| d.get_temp::<Cache>(id));
    let texture = if let Some(cache) = cached.filter(|c| c.key == key) {
        cache.texture
    } else {
        let texture = ui.ctx().load_texture(
            "LDtk selection PixelOutline",
            raster(&marks, rect, size),
            egui::TextureOptions::NEAREST,
        );
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                id,
                Cache {
                    key,
                    texture: texture.clone(),
                },
            )
        });
        texture
    };
    ui.painter().with_clip_rect(canvas).image(
        texture.id(),
        Rect::from_min_max(rect.min / ppp, rect.max / ppp),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        Color32::WHITE,
    );
}

impl EditorApp {
    pub(super) fn draw_selection_highlight(&self, ui: &egui::Ui, rect: Rect) {
        if self.world
            || matches!(
                self.gesture,
                Some(
                    tools::Gesture::SelectionMove { moved: true, .. }
                        | tools::Gesture::EntityMove { moved: true, .. }
                )
            )
        {
            return;
        }
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        if let Some(group) = &self.selection_group {
            paint_marks(
                ui,
                rect,
                &self.viewport,
                &group_marks(project, key, &self.viewport, group),
            );
        } else if let Some(entity) = self
            .entity_index
            .and_then(|i| project.layer(key, self.layer)?["entityInstances"].get(i))
        {
            let layer = project.layer(key, self.layer).unwrap();
            let group = serde_json::json!({"layers":[{"layerDefUid":layer["layerDefUid"],"clipboard":{"entityInstances":[{"iid":entity["iid"]}]}}]});
            paint_marks(
                ui,
                rect,
                &self.viewport,
                &group_marks(project, key, &self.viewport, &group),
            );
        }
    }
    pub(super) fn draw_entity_move_arrow(&self, ui: &egui::Ui, rect: Rect) {
        let Some(tools::Gesture::EntityMove {
            key,
            layer,
            start,
            end,
            moved: true,
            ..
        }) = &self.gesture
        else {
            return;
        };
        let Some(project) = &self.project else { return };
        let Some(instance) = project.layer(*key, *layer) else {
            return;
        };
        let grid = number(instance, "__gridSize", 16.);
        let center = |p: [f32; 2]| {
            canvas::layer_to_world(
                project,
                *key,
                *layer,
                &self.viewport,
                p.map(|v| ((v / grid).floor() + 0.5) * grid),
            )
        };
        draw_move_arrow(
            ui,
            rect,
            &self.viewport,
            center(*start),
            center(*end),
            false,
        );
    }
}
pub(super) fn draw_move_arrow(
    ui: &egui::Ui,
    rect: Rect,
    view: &Viewport,
    from: [f32; 2],
    to: [f32; 2],
    copy: bool,
) {
    let a: egui::Pos2 = view
        .to_screen(from, rect.min.into(), rect.size().into())
        .into();
    let b: egui::Pos2 = view
        .to_screen(to, rect.min.into(), rect.size().into())
        .into();
    let d = b - a;
    if d.length_sq() < 0.01 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let color = if copy {
        Color32::from_rgb(255, 204, 0)
    } else {
        Color32::WHITE
    };
    let normal = egui::vec2(-d.y, d.x).normalized() * view.zoom;
    if copy {
        for sign in [-1., 1.] {
            painter.line_segment(
                [a + normal * sign, b + normal * sign],
                Stroke::new(view.zoom, color),
            );
        }
    } else {
        painter.line_segment([a, b], Stroke::new(view.zoom, color));
    }
    for sign in [-1., 1.] {
        let p = b + Vec2::angled(d.angle() + sign * std::f32::consts::PI * 0.8) * (6. * view.zoom);
        painter.line_segment([b, p], Stroke::new(2. * view.zoom, color));
    }
    painter.circle_filled(b, view.zoom, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn sparse_selected_cells_leave_the_hole_unmarked_and_points_are_circles() {
        let mut p = Project::new();
        p.add_layer("Ground", "IntGrid", 16).unwrap();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let actor = p.add_entity_def("Actor", 16, 32).unwrap();
        p.update_entity_definition(actor, &json!({"pivotX":0.5,"pivotY":1.}))
            .unwrap();
        let point = p
            .add_field_definition(Some(actor), "Aim", "Point", false)
            .unwrap();
        let key = p.level_keys()[0];
        let entity_layer = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["__type"] == "Entities")
            .unwrap();
        let grid_layer = 1 - entity_layer;
        p.add_entity(key, entity_layer, actor, 80, 80).unwrap();
        let iid = p.layer(key, entity_layer).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_editor_value(key, Some(&iid), point, json!({"cx":7,"cy":3}))
            .unwrap();
        p.paint_int_grid(key, grid_layer, 0, 0, 1).unwrap();
        p.paint_int_grid(key, grid_layer, 2, 0, 1).unwrap();
        let group = p
            .copy_selection_group(key, &[grid_layer, entity_layer], [0, 0, 144, 96])
            .unwrap();
        let marks = group_marks(&p, key, &Viewport::default(), &group);
        assert!(marks.contains(&Mark::Rect(
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(16., 16.)),
            YELLOW
        )));
        assert!(
            !marks
                .iter()
                .any(|m| matches!(m,Mark::Rect(r,_) if r.contains(egui::pos2(24.,8.))))
        );
        assert!(marks.contains(&Mark::Rect(
            Rect::from_min_max(egui::pos2(72., 48.), egui::pos2(88., 80.)),
            YELLOW
        )));
        assert!(marks.contains(&Mark::Circle(egui::pos2(120., 56.), 6.4, YELLOW)));
    }
    #[test]
    fn selection_shapes_apply_layer_offsets_parallax_scale_and_original_rects() {
        let mut p = Project::new();
        let uid = p.add_layer("Ground", "IntGrid", 16).unwrap();
        let key = p.level_keys()[0];
        p.update_layer_properties(uid, &json!({"pxOffsetX":24,"pxOffsetY":-8,"parallaxFactorX":0.5,"parallaxFactorY":0.5,"parallaxScaling":true})).unwrap();
        p.paint_int_grid(key, 0, 2, 3, 1).unwrap();
        let mut group = p.select_all_group(key, &[0]).unwrap();
        group["originalRects"] = json!([{"leftPx":10,"topPx":20,"rightPx":30,"bottomPx":40}]);
        let view = Viewport {
            center: [200., 100.],
            ..Viewport::default()
        };
        let marks = group_marks(&p, key, &view, &group);
        assert!(marks.contains(&Mark::Rect(
            Rect::from_min_max(egui::pos2(128., 70.), egui::pos2(136., 78.)),
            YELLOW
        )));
        assert!(marks.contains(&Mark::Rect(
            Rect::from_min_max(egui::pos2(10., 20.), egui::pos2(30., 40.)),
            BLUE
        )));
    }
    #[test]
    fn four_neighbour_outline_has_no_diagonal_and_partial_knockout_precedes_black() {
        let mut pixels = vec![[0_u8; 4]; 49];
        pixels[3 * 7 + 3] = [255, 204, 0, 255];
        let yellow = pixel_outline(&pixels, [7, 7], [1., 0.8, 0.], 0.66);
        assert_eq!(yellow[3 * 7 + 2], [255, 204, 0, 255]);
        assert_eq!(yellow[2 * 7 + 2], [0; 4]);
        assert_eq!(yellow[3 * 7 + 3][3], 168);
        let black = pixel_outline(&yellow, [7, 7], [0.; 3], 1.);
        assert_eq!(black[3 * 7 + 1], [0, 0, 0, 255]);
        assert_eq!(black[7 + 1], [0; 4]);
        assert_eq!(black[3 * 7 + 3][3], 168);
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    #[test]
    fn touching_shapes_have_no_internal_outline_and_real_holes_remain_transparent() {
        let image = raster(
            &[
                Mark::Rect(
                    Rect::from_min_max(egui::pos2(4., 4.), egui::pos2(12., 12.)),
                    YELLOW,
                ),
                Mark::Rect(
                    Rect::from_min_max(egui::pos2(12., 4.), egui::pos2(20., 12.)),
                    YELLOW,
                ),
                Mark::Circle(egui::pos2(28., 8.), 2., YELLOW),
            ],
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(36., 16.)),
            [36, 16],
        );
        assert_eq!(image[(11, 8)], image[(12, 8)]);
        assert_eq!(image[(12, 8)], image[(8, 8)]);
        assert_eq!(image[(23, 8)], Color32::TRANSPARENT);
        assert_eq!(image[(2, 8)], Color32::BLACK);
        assert_eq!(image[(3, 8)], Color32::from_rgb(255, 204, 0));
        assert_eq!(image[(28, 8)].a(), 168);
        assert_eq!(image[(25, 6)], Color32::BLACK);
    }
    #[test]
    fn stationary_selection_reuses_uploaded_texture_and_panning_invalidates_it() {
        let ctx = egui::Context::default();
        let mut view = Viewport {
            zoom: 1.,
            center: [16., 16.],
        };
        let marks = [Mark::Rect(
            Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(32., 32.)),
            YELLOW,
        )];
        let run = |view: &Viewport| {
            ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    paint_marks(
                        ui,
                        Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(100.)),
                        view,
                        &marks,
                    )
                });
            })
        };
        let first = run(&view);
        let second = run(&view);
        assert!(!first.textures_delta.set.is_empty());
        assert!(second.textures_delta.set.is_empty());
        view.center[0] += 1.;
        let moved = run(&view);
        assert_eq!(moved.textures_delta.set.len(), 1);
    }
    #[test]
    fn entity_move_arrow_uses_mouse_cells_and_source_white_wings() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let key = project.level_keys()[0];
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.viewport.center = [0., 0.];
        app.viewport.zoom = 2.;
        app.gesture = Some(tools::Gesture::EntityMove {
            key,
            layer: 0,
            index: 0,
            start: [1., 1.],
            end: [33., 1.],
            origin: [0, 0],
            moved: true,
        });
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.draw_entity_move_arrow(
                    ui,
                    Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(200.)),
                )
            });
        });
        let lines = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::LineSegment { points, stroke } => Some((points, stroke)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0].0,
            &[egui::pos2(116., 116.), egui::pos2(180., 116.)]
        );
        assert_eq!(lines[0].1.color, Color32::WHITE);
        assert_eq!(lines[1].1.width, 4.);
    }
}
