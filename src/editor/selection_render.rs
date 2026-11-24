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
    xs: &[f32],
    ys: &[f32],
    color: [f32; 3],
    knockout: f32,
) -> Vec<[u8; 4]> {
    let (width, height) = (xs.len(), ys.len());
    let consecutive = |axis: &[f32]| {
        axis.windows(2)
            .map(|pair| pair[1] - pair[0] == 1.)
            .collect::<Vec<_>>()
    };
    let x_steps = consecutive(xs);
    let y_steps = consecutive(ys);
    input
        .iter()
        .enumerate()
        .map(|(index, current)| {
            if current[3] == 255 && knockout == 1. {
                return *current;
            }
            let x = index % width;
            let y = index / width;
            let mut neighbour = 0_f32;
            for (valid, other) in [
                (x > 0 && x_steps[x - 1], index.wrapping_sub(1)),
                (x + 1 < width && x_steps[x], index + 1),
                (y > 0 && y_steps[y - 1], index.wrapping_sub(width)),
                (y + 1 < height && y_steps[y], index + width),
            ] {
                if valid {
                    neighbour = neighbour.max(f32::from(input[other][3]) / 255.);
                }
            }
            if current[3] == 0 && neighbour == 0. {
                return [0; 4];
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
fn raster_pixels(marks: &[Mark], xs: &[f32], ys: &[f32]) -> Vec<[u8; 4]> {
    let mut pixels = vec![[0_u8; 4]; xs.len() * ys.len()];
    for &mark in marks {
        let b = bounds(mark);
        if !b.is_positive() {
            continue;
        }
        let min_y = ys.partition_point(|&y| y < b.top());
        let max_y = ys.partition_point(|&y| y <= b.bottom());
        if min_y == max_y {
            continue;
        }
        let color = match mark {
            Mark::Rect(_, c) | Mark::Circle(_, _, c) => c,
        };
        let opaque = (color[3] == 1.).then(|| color.map(|channel| (channel * 255.).round() as u8));
        let min_x = xs.partition_point(|&x| x < b.left());
        let max_x = xs.partition_point(|&x| x <= b.right());
        for (row, &y) in ys[min_y..max_y].iter().enumerate() {
            let start = (row + min_y) * xs.len() + min_x;
            let end = start + max_x - min_x;
            for (&x, current) in xs[min_x..max_x].iter().zip(&mut pixels[start..end]) {
                let p = egui::pos2(x, y);
                let inside = match mark {
                    Mark::Rect(r, _) => r.contains(p),
                    Mark::Circle(c, r, _) => c.distance_sq(p) <= r * r,
                };
                if inside {
                    if let Some(opaque) = opaque {
                        *current = opaque;
                    } else {
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
    }
    let first = pixel_outline(&pixels, xs, ys, [1., 0.8, 0.], 0.66);
    drop(pixels);
    pixel_outline(&first, xs, ys, [0.; 3], 1.)
}

fn premultiplied([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_premultiplied(r, g, b, a)
}

fn raster(marks: &[Mark], rect: Rect, size: [usize; 2]) -> egui::ColorImage {
    let xs: Vec<_> = (0..size[0]).map(|x| rect.min.x + x as f32 + 0.5).collect();
    let ys: Vec<_> = (0..size[1]).map(|y| rect.min.y + y as f32 + 0.5).collect();
    egui::ColorImage::new(
        size,
        raster_pixels(marks, &xs, &ys)
            .into_iter()
            .map(premultiplied)
            .collect(),
    )
}

// Two outline passes need the sampled level texel and its neighbours out to
// two pixels. Skip the gaps between these neighbourhoods when zoomed out.
fn sample_axis(start: f32, step: f32, len: usize) -> (Vec<f32>, Vec<usize>) {
    let centers: Vec<_> = (0..len)
        .map(|i| (start + i as f32 * step).floor() + 0.5)
        .collect();
    let mut axis = Vec::with_capacity(len);
    for &center in &centers {
        for offset in -2..=2 {
            let texel = center + offset as f32;
            if axis.last().is_none_or(|&last| last < texel) {
                axis.push(texel);
            }
        }
    }
    let samples = centers
        .iter()
        .map(|center| {
            axis.binary_search_by(|texel| texel.total_cmp(center))
                .unwrap()
        })
        .collect();
    (axis, samples)
}

fn raster_sampled(
    marks: &[Mark],
    size: [usize; 2],
    first: egui::Pos2,
    step: f32,
) -> egui::ColorImage {
    let (xs, columns) = sample_axis(first.x, step, size[0]);
    let mut pixels = Vec::with_capacity(size[0] * size[1]);
    // Process neighbouring rows together, with a fresh two-pixel margin on
    // each strip, so temporary storage stays bounded by the window width.
    const STRIP_ROWS: usize = 16;
    for top in (0..size[1]).step_by(STRIP_ROWS) {
        let count = STRIP_ROWS.min(size[1] - top);
        let (ys, rows) = sample_axis(first.y + top as f32 * step, step, count);
        let strip = raster_pixels(marks, &xs, &ys);
        for row in rows {
            let start = row * xs.len();
            pixels.extend(
                columns
                    .iter()
                    .map(|column| premultiplied(strip[start + column])),
            );
        }
    }
    egui::ColorImage::new(size, pixels)
}
#[derive(Clone)]
struct Cache {
    key: u64,
    texture: egui::TextureHandle,
}

fn raster_bounds(marks: &[Mark], visible: Rect) -> Rect {
    let rect = marks
        .iter()
        .fold(Rect::NOTHING, |r, m| r.union(bounds(*m)))
        .expand(2.)
        .intersect(visible.expand(2.));
    Rect::from_min_max(rect.min.floor(), rect.max.ceil())
}

fn paint_marks(ui: &egui::Ui, canvas: Rect, view: &Viewport, marks: &[Mark]) {
    use std::hash::{Hash, Hasher};
    let ppp = ui.ctx().pixels_per_point();
    let origin = view.level_render_canvas_origin(canvas, ppp);
    let on_screen = |p: egui::Pos2| -> egui::Pos2 {
        view.to_screen(p.into(), origin, canvas.size().into())
            .into()
    };
    let in_level = |p: egui::Pos2| -> egui::Pos2 {
        view.to_world(p.into(), origin, canvas.size().into()).into()
    };
    let level_rect = raster_bounds(
        marks,
        Rect::from_min_max(in_level(canvas.min), in_level(canvas.max)),
    );
    if !level_rect.is_finite() || !level_rect.is_positive() {
        return;
    }
    let max_side = ui.input(|input| input.max_texture_side) as f32;
    let canvas_pixels = canvas.area() * ppp * ppp;
    let projected = Rect::from_min_max(on_screen(level_rect.min), on_screen(level_rect.max));
    let (rect, screen_rect, sampled) = if level_rect.size().max_elem() <= max_side
        && level_rect.area() <= canvas_pixels.max(1024. * 1024.)
    {
        (level_rect, projected, None)
    } else {
        let pixel_scale = ppp.min((max_side - 2.).max(1.) / canvas.size().max_elem());
        let visible = projected.intersect(canvas);
        let rect = Rect::from_min_max(
            (visible.min * pixel_scale).floor(),
            (visible.max * pixel_scale).ceil(),
        );
        (
            rect,
            Rect::from_min_max(rect.min / pixel_scale, rect.max / pixel_scale),
            Some((
                in_level((rect.min + Vec2::splat(0.5)) / pixel_scale),
                1. / (pixel_scale * view.zoom),
            )),
        )
    };
    if !rect.is_finite() || !rect.is_positive() {
        return;
    }
    let size = [rect.width() as usize, rect.height() as usize];
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for n in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
        n.to_bits().hash(&mut hasher);
    }
    sampled.is_some().hash(&mut hasher);
    if let Some((first, step)) = sampled {
        for n in [first.x, first.y, step] {
            n.to_bits().hash(&mut hasher);
        }
    }
    for m in marks {
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
        let image = match sampled {
            Some((first, step)) => raster_sampled(marks, size, first, step),
            None => raster(marks, rect, size),
        };
        let texture = ui.ctx().load_texture(
            "LDtk selection PixelOutline",
            image,
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
        screen_rect,
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
            let layer_pixel = canvas::pointer_to_layer(project, *key, *layer, &self.viewport, p);
            canvas::layer_to_pointer_world(
                project,
                *key,
                *layer,
                &self.viewport,
                layer_pixel.map(|v| ((v / grid).floor() + 0.5) * grid),
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
    let origin = view.level_render_canvas_origin(rect, ui.ctx().pixels_per_point());
    let a: egui::Pos2 = view.to_screen(from, origin, rect.size().into()).into();
    let b: egui::Pos2 = view.to_screen(to, origin, rect.size().into()).into();
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
        let axis = [0., 1., 2., 3., 4., 5., 6.];
        let yellow = pixel_outline(&pixels, &axis, &axis, [1., 0.8, 0.], 0.66);
        assert_eq!(yellow[3 * 7 + 2], [255, 204, 0, 255]);
        assert_eq!(yellow[2 * 7 + 2], [0; 4]);
        assert_eq!(yellow[3 * 7 + 3][3], 168);
        let black = pixel_outline(&yellow, &axis, &axis, [0.; 3], 1.);
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
    fn selection_outline_is_rasterized_in_level_pixels_and_reused_until_clipped() {
        let ctx = egui::Context::default();
        let mut view = Viewport {
            zoom: 2.,
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
        assert!(
            first
                .textures_delta
                .set
                .iter()
                .any(|(_, delta)| delta.image.size() == [36, 36]),
            "32 level pixels plus two one-pixel outline passes"
        );
        assert!(second.textures_delta.set.is_empty());
        view.center[0] += 1.;
        view.zoom = 2.5;
        let moved = run(&view);
        assert!(moved.textures_delta.set.is_empty());
        view.center[0] = 48.;
        let clipped = run(&view);
        assert_eq!(clipped.textures_delta.set.len(), 1);
    }
    #[test]
    fn fit_zoom_projects_selection_from_the_rounded_backing_origin() {
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(egui::pos2(300., 0.), egui::vec2(1140., 900.));
        let view = Viewport {
            center: [1024.; 2],
            zoom: 900. / 2128.,
        };
        let marks = [Mark::Rect(
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(32.)),
            YELLOW,
        )];
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint_marks(ui, canvas, &view, &marks));
        });
        let first_vertex = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) => Some(mesh.vertices[0].pos),
                _ => None,
            })
            .unwrap();
        let filter_margin = Vec2::splat(2. * view.zoom);
        assert!((first_vertex + filter_margin).distance(egui::pos2(437., 17.)) < 0.0001);
    }
    #[test]
    fn sampled_outline_matches_dense_outline_across_strips_and_gaps() {
        let marks = [
            Mark::Rect(
                Rect::from_min_max(egui::pos2(-2., -30.), egui::pos2(23., 250.)),
                BLUE,
            ),
            Mark::Rect(
                Rect::from_min_max(egui::pos2(20., -1.), egui::pos2(45., 101.)),
                [BLUE[0], BLUE[1], BLUE[2], 0.6],
            ),
            Mark::Circle(egui::pos2(25., 58.), 8.4, YELLOW),
        ];
        let area = Rect::from_min_size(egui::pos2(-12., -44.), egui::vec2(110., 360.));
        let dense = raster(&marks, area, [110, 360]);
        let first = egui::pos2(-4.4, -37.5);
        let size = [12, 45];
        for step in [1.25, 3.25, 7.5] {
            let sampled = raster_sampled(&marks, size, first, step);
            for row in 0..size[1] {
                for column in 0..size[0] {
                    let at = first + egui::vec2(column as f32, row as f32) * step - area.min;
                    assert_eq!(
                        sampled[(column, row)],
                        dense[(at.x.floor() as usize, at.y.floor() as usize)],
                        "step={step}, column={column}, row={row}"
                    );
                }
            }
        }
    }

    #[test]
    fn oversized_selection_samples_the_same_subpixel_outline_as_a_small_selection() {
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(100.));
        let view = Viewport {
            center: [0., 0.],
            zoom: 0.025,
        };
        let marks = [Mark::Rect(
            Rect::from_min_max(egui::pos2(0., -1300.), egui::pos2(2000., 1300.)),
            YELLOW,
        )];
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint_marks(ui, canvas, &view, &marks));
        });
        let image = output
            .textures_delta
            .set
            .iter()
            .find_map(|(id, delta)| {
                if *id == egui::TextureId::default() {
                    return None;
                }
                let egui::ImageData::Color(image) = &delta.image;
                Some(image)
            })
            .unwrap();
        assert!(image.width() <= 100 && image.height() <= 100);
        let middle = &image.pixels[(image.height() / 2) * image.width()..];
        let first_visible = middle
            .iter()
            .take(image.width())
            .find(|c| c.a() != 0)
            .unwrap();
        assert_eq!(
            first_visible.a(),
            168,
            "the first visible pixel samples the interior"
        );
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
            press: egui::Pos2::ZERO,
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
