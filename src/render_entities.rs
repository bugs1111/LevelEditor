use super::{
    RenderOptions, Scene, Transform, geometry, lighten, number, pair, text, tile_anchor, values,
};
use crate::{assets::Assets, theme, viewport::Viewport};
use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Stroke, StrokeKind, vec2};
use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
use std::{collections::HashSet, ops::Range};

fn visible_cell_range(
    cells: Range<i64>,
    grid: f32,
    project: impl Fn(f32) -> f32,
    min: f32,
    max: f32,
) -> Range<i64> {
    let cell_start = |index: i64| index as f32 * grid;
    let first = |predicate: &dyn Fn(i64) -> bool| {
        let (mut lo, mut hi) = (cells.start, cells.end);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if predicate(mid) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        lo
    };
    // Rect::intersects includes cells that touch the viewport boundary.
    first(&|i| project(cell_start(i) + grid) >= min)..first(&|i| project(cell_start(i)) > max)
}

#[derive(Default)]
struct WorldCells {
    visible: Option<VisibleWorldCells>,
    other: HashSet<(i32, i32, i32)>,
}

struct VisibleWorldCells {
    grid: i32,
    columns: Range<i64>,
    rows: Range<i64>,
    bits: Vec<u64>,
}

impl VisibleWorldCells {
    fn slot(&self, (grid, x, y): (i32, i32, i32)) -> Option<(usize, u64)> {
        let (x, y) = (i64::from(x), i64::from(y));
        if grid != self.grid || !self.columns.contains(&x) || !self.rows.contains(&y) {
            return None;
        }
        let width = self.columns.end - self.columns.start;
        let index = ((y - self.rows.start) * width + (x - self.columns.start)) as usize;
        Some((index / 64, 1 << (index % 64)))
    }
}

impl WorldCells {
    fn track_visible(&mut self, grid: i32, columns: Range<i64>, rows: Range<i64>) {
        if self.visible.is_some() || columns.is_empty() || rows.is_empty() {
            return;
        }
        let area = (columns.end - columns.start).saturating_mul(rows.end - rows.start);
        if !(1..=1_048_576).contains(&area) {
            return;
        }
        let mut visible = VisibleWorldCells {
            grid,
            columns,
            rows,
            bits: vec![0; (area as usize).div_ceil(64)],
        };
        // Textured layers or neighbor edges may already cover cells inside this region.
        self.other.retain(|&cell| {
            let Some((word, bit)) = visible.slot(cell) else {
                return true;
            };
            visible.bits[word] |= bit;
            false
        });
        self.visible = Some(visible);
    }

    fn contains(&self, cell: (i32, i32, i32)) -> bool {
        if let Some(visible) = &self.visible
            && let Some((word, bit)) = visible.slot(cell)
        {
            return visible.bits[word] & bit != 0;
        }
        self.other.contains(&cell)
    }

    fn insert(&mut self, cell: (i32, i32, i32)) -> bool {
        if let Some(visible) = &mut self.visible
            && let Some((word, bit)) = visible.slot(cell)
        {
            let bits = &mut visible.bits[word];
            let added = *bits & bit == 0;
            *bits |= bit;
            return added;
        }
        self.other.insert(cell)
    }
}

pub(super) fn entity_bounds(entity: &Value, def: &Value) -> Rect {
    let size = vec2(
        number(entity, "width", number(def, "width", 16.)),
        number(entity, "height", number(def, "height", 16.)),
    );
    let pivot = pair(
        &entity["__pivot"],
        [number(def, "pivotX", 0.), number(def, "pivotY", 0.)],
    );
    let pos = pair(&entity["px"], [0.; 2]);
    Rect::from_min_size(Pos2::from(pos) - size * vec2(pivot[0], pivot[1]), size)
}
pub(crate) fn entity_smart_color(
    project: &Project,
    entity: &Value,
    def: &Value,
    bright: bool,
) -> Color32 {
    for fd in values(&def["fieldDefs"]) {
        if fd["useForSmartColor"] != true {
            continue;
        }
        let value = super::fields::field_value(entity, fd);
        let items = if fd["isArray"] == true {
            values(&value)
        } else {
            std::slice::from_ref(value.as_ref())
        };
        for value in items {
            if value.is_null() {
                continue;
            }
            if text(fd, "__type", "").contains("Color") {
                if let Some(color) = value.as_str() {
                    return theme::color(color);
                }
            } else if let Some(ev) = super::fields::enum_value(project, fd, value)
                && let Some(color) = ev["color"].as_str()
            {
                return theme::color(color);
            }
        }
    }
    let c = theme::color(text(def, "color", text(entity, "__smartColor", "#ffcc00")));
    if bright { lighten(c, 0.5) } else { c }
}
pub(crate) fn entity_field_tile(project: &Project, entity: &Value, def: &Value) -> Option<Value> {
    for fd in values(&def["fieldDefs"]) {
        if fd["editorDisplayMode"] == "EntityTile" {
            let value = super::fields::field_value(entity, fd);
            let first = if fd["isArray"] == true {
                &value[0]
            } else {
                &value
            };
            if let Some(tile) = super::fields::value_icon(project, fd, first) {
                return Some(tile);
            }
        }
    }
    None
}

impl Scene<'_> {
    pub(super) fn smart_tile(&self, entity: &Value, def: &Value) -> Option<Value> {
        if let Some(tile) = entity_field_tile(self.project, entity, def) {
            return Some(tile);
        }
        if !def["tileRect"].is_null() {
            Some(def["tileRect"].clone())
        } else if !entity["__tile"].is_null() {
            Some(entity["__tile"].clone())
        } else {
            None
        }
    }
    pub(super) fn smart_color(&self, entity: &Value, def: &Value) -> Color32 {
        self.smart_color_bright(entity, def, false)
    }
    pub(super) fn smart_color_bright(&self, entity: &Value, def: &Value, bright: bool) -> Color32 {
        entity_smart_color(self.project, entity, def, bright)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn entity(
        &mut self,
        painter: &Painter,
        entity: &Value,
        layer_def: &Value,
        transform: Transform,
        layer_alpha: f32,
        active: bool,
        key: LevelKey,
    ) {
        let def = self.project.json["defs"]["entities"]
            .as_array()
            .and_then(|a| a.iter().find(|d| d["uid"] == entity["defUid"]))
            .unwrap_or(&Value::Null);
        let bounds = entity_bounds(entity, def);
        let rect = self.screen(transform.rect(bounds));
        let visible = rect
            .expand(300. * self.options.ui_scale)
            .intersects(self.canvas);
        if !visible
            && !values(&def["fieldDefs"]).iter().any(|field| {
                (def["tileRenderMode"] == "FullSizeUncropped"
                    && field["__type"] == "FilePath"
                    && field["isArray"] != true
                    && field["editorDisplayMode"] == "EntityTile")
                    || matches!(
                        text(field, "editorDisplayMode", ""),
                        "RadiusPx"
                            | "RadiusGrid"
                            | "RefLinkBetweenCenters"
                            | "RefLinkBetweenPivots"
                            | "Points"
                            | "PointStar"
                            | "PointPath"
                            | "PointPathLoop"
                    )
            })
        {
            return;
        }
        let image_tile = values(&def["fieldDefs"]).iter().find_map(|field| {
            if field["__type"] != "FilePath"
                || field["isArray"] == true
                || field["editorDisplayMode"] != "EntityTile"
            {
                return None;
            }
            let value = super::fields::field_value(entity, field);
            let path = crate::canvas::asset_path(self.project, value.as_str()?);
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            if !matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "ase" | "aseprite"
            ) || !path.is_file()
            {
                return None;
            }
            self.assets.texture(self.ctx, &path)
        });
        let color = self.smart_color(entity, def);
        // Entities can overhang their layer; only the viewport clips them.
        let core_alpha = layer_alpha
            * if active {
                1.
            } else {
                number(layer_def, "inactiveOpacity", 1.)
            };
        let tile = self.smart_tile(entity, def);
        let fill = if def["hollow"] == true {
            Color32::TRANSPARENT
        } else {
            color.gamma_multiply(number(def, "fillOpacity", 1.) * core_alpha)
        };
        let stroke = Stroke::new(
            1.0_f32,
            lighten(color, 0.3).gamma_multiply(number(def, "lineOpacity", 1.) * core_alpha),
        );
        let visible = rect.intersects(self.canvas);
        let mode = text(def, "renderMode", "Rectangle");
        if visible {
            if tile.is_some() || image_tile.is_some() || mode == "Rectangle" {
                painter.rect(rect, 0., fill, stroke, StrokeKind::Middle);
            } else if mode == "Ellipse" {
                let segments = if bounds.width() <= 16. || bounds.height() <= 16. {
                    16
                } else {
                    64
                };
                let points = (0..segments)
                    .map(|i| {
                        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
                        rect.center()
                            + vec2(a.cos() * rect.width() * 0.5, a.sin() * rect.height() * 0.5)
                    })
                    .collect();
                painter.add(egui::Shape::convex_polygon(points, fill, stroke));
            } else if mode == "Cross" {
                let stroke = Stroke::new(
                    5.0_f32,
                    color.gamma_multiply(number(def, "lineOpacity", 1.) * core_alpha),
                );
                painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
                painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
            }
        }
        let tile_texture = if let Some(texture) = image_tile {
            let source = Rect::from_min_size(Pos2::ZERO, texture.size_vec2());
            Some((texture, source))
        } else {
            tile.as_ref().and_then(|tile| {
                let atlas = tile["tilesetUid"]
                    .as_i64()
                    .and_then(|uid| self.project.tileset(uid))?;
                let texture = self.texture(atlas)?;
                let source = Rect::from_min_size(
                    [number(tile, "x", 0.), number(tile, "y", 0.)].into(),
                    vec2(number(tile, "w", 16.), number(tile, "h", 16.)),
                );
                Some((texture, source))
            })
        };
        if visible {
            if tile_texture.is_none() && (tile.is_some() || mode == "Tile") {
                missing_tile(painter, rect);
            }
            // The pivot is part of the base drawing beneath the tile texture.
            let pivot = self.point(transform.point(pair(&entity["px"], [0.; 2])));
            let pixel = self.view.zoom * transform.scale;
            painter.rect_filled(
                Rect::from_min_size(pivot - vec2(pixel, pixel), vec2(pixel * 3., pixel * 3.)),
                0.,
                Color32::BLACK.gamma_multiply(core_alpha * 0.4),
            );
            painter.rect_filled(
                Rect::from_min_size(pivot, vec2(pixel, pixel)),
                0.,
                color.gamma_multiply(core_alpha),
            );
        }
        if let Some((texture, source)) = tile_texture {
            let pivot = pair(
                &entity["__pivot"],
                [number(def, "pivotX", 0.), number(def, "pivotY", 0.)],
            );
            let borders = [
                number_at(&def["nineSliceBorders"], 0),
                number_at(&def["nineSliceBorders"], 1),
                number_at(&def["nineSliceBorders"], 2),
                number_at(&def["nineSliceBorders"], 3),
            ];
            let a = self.view.to_world(
                self.canvas.min.into(),
                self.canvas.min.into(),
                self.canvas.size().into(),
            );
            let b = self.view.to_world(
                self.canvas.max.into(),
                self.canvas.min.into(),
                self.canvas.size().into(),
            );
            let clip = Rect::from_min_max(
                [
                    (a[0] - transform.origin[0]) / transform.scale,
                    (a[1] - transform.origin[1]) / transform.scale,
                ]
                .into(),
                [
                    (b[0] - transform.origin[0]) / transform.scale,
                    (b[1] - transform.origin[1]) / transform.scale,
                ]
                .into(),
            );
            let quads = geometry::entity_tiles_clipped(
                bounds,
                source,
                pivot,
                text(def, "tileRenderMode", "FitInside"),
                borders,
                clip,
            );
            let mut mesh = Mesh::with_texture(texture.id());
            let size = texture.size_vec2();
            for q in quads {
                mesh.add_rect_with_uv(
                    self.screen(transform.rect(q.dst)),
                    Rect::from_min_max(
                        (q.src.min.to_vec2() / size).to_pos2(),
                        (q.src.max.to_vec2() / size).to_pos2(),
                    ),
                    Color32::WHITE.gamma_multiply(core_alpha * number(def, "tileOpacity", 1.)),
                );
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        if self.options.details && (active || layer_def["hideFieldsWhenInactive"] != true) {
            self.entity_fields(
                painter,
                entity,
                def,
                layer_def,
                transform,
                color,
                key,
                layer_alpha,
                active,
            );
        }
    }
    pub(super) fn world_layers(
        &mut self,
        painter: &Painter,
        level: &Value,
        origin: [f32; 2],
        edge_region: Option<Rect>,
    ) -> usize {
        let clip = painter.clip_rect();
        let mut done = WorldCells::default();
        let screen_center: [f32; 2] =
            (Pos2::from(self.projection_origin) + self.canvas.size() * 0.5).into();
        let center = self.view.center;
        let zoom = self.view.zoom;
        if let Some(region) = edge_region {
            for layer in values(&level["layerInstances"]) {
                let Some(atlas) = layer["__tilesetDefUid"]
                    .as_i64()
                    .and_then(|uid| self.project.tileset(uid))
                else {
                    continue;
                };
                if self.texture(atlas).is_none() {
                    continue;
                }
                let grid = number(layer, "__gridSize", 16.).max(1.);
                if layer["__type"] == "Tiles" {
                    let width = number(layer, "__cWid", 0.) as i32;
                    let height = number(layer, "__cHei", 0.) as i32;
                    let x0 = (region.left() / grid).ceil().max(0.) as i32;
                    let y0 = (region.top() / grid).ceil().max(0.) as i32;
                    let x1 = (region.right() / grid).ceil().min(width as f32) as i32;
                    let y1 = (region.bottom() / grid).ceil().min(height as f32) as i32;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            done.insert((grid as i32, x, y));
                        }
                    }
                }
                for tile in values(&layer["autoLayerTiles"]) {
                    let px = pair(tile_anchor(tile), [0.; 2]);
                    if px[0] >= region.left()
                        && px[0] < region.right()
                        && px[1] >= region.top()
                        && px[1] < region.bottom()
                    {
                        done.insert((grid as i32, (px[0] / grid) as i32, (px[1] / grid) as i32));
                    }
                }
            }
        }
        let mut count = 0;
        for layer in values(&level["layerInstances"]) {
            let uid = layer["layerDefUid"].as_i64().unwrap_or(-1);
            let def = self.project.layer_def(uid).unwrap_or(&Value::Null);
            if layer["__type"] == "Entities" || def["renderInWorldView"] == false {
                continue;
            }
            let grid = number(layer, "__gridSize", 16.).max(1.);
            let transform = Transform { origin, scale: 1. }.offset([
                number(layer, "__pxTotalOffsetX", 0.),
                number(layer, "__pxTotalOffsetY", 0.),
            ]);
            let project_axis = |axis: usize, point: f32| {
                let world = transform.origin[axis] + point * transform.scale;
                screen_center[axis] + (world - center[axis]) * zoom
            };
            if layer["__type"] == "IntGrid" && layer["__tilesetDefUid"].is_null() {
                let width = number(layer, "__cWid", 0.) as usize;
                if width == 0 {
                    continue;
                }
                let cells = values(&layer["intGridCsv"]);
                let columns = visible_cell_range(
                    0..width.min(cells.len()) as i64,
                    grid,
                    |x| project_axis(0, x),
                    clip.left(),
                    clip.right(),
                );
                let rows = visible_cell_range(
                    0..cells.len().div_ceil(width) as i64,
                    grid,
                    |y| project_axis(1, y),
                    clip.top(),
                    clip.bottom(),
                );
                done.track_visible(grid as i32, columns.clone(), rows.clone());
                let visible_cells = rows.flat_map(|y| {
                    let start = y as usize * width;
                    let row = &cells[start..(start + width).min(cells.len())];
                    let py = y as f32 * grid;
                    let top = project_axis(1, py);
                    let bottom = project_axis(1, py + grid);
                    (columns.start..columns.end.min(row.len() as i64))
                        .map(move |x| (x, y, top, bottom, row[x as usize].as_i64().unwrap_or(0)))
                });
                let mut color = super::intgrid_cell_colors(def, 1.);
                let shapes = visible_cells.filter_map(|(x, y, top, bottom, value)| {
                    if value == 0 || !done.insert((grid as i32, x as i32, y as i32)) {
                        return None;
                    }
                    let px = x as f32 * grid;
                    let dst = Rect::from_min_max(
                        egui::pos2(project_axis(0, px), top),
                        egui::pos2(project_axis(0, px + grid), bottom),
                    );
                    count += 1;
                    Some(egui::Shape::rect_filled(dst, 0., color(value)))
                });
                if painter.opacity() == 0. || !painter.is_visible() {
                    for shape in shapes {
                        painter.add(shape);
                    }
                } else {
                    painter.extend(shapes);
                }
                continue;
            }
            let Some(atlas) = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| self.project.tileset(uid))
            else {
                continue;
            };
            let colors = text(&atlas["cachedPixelData"], "averageColors", "");
            if self.texture(atlas).is_none() {
                continue;
            }
            let atlas_path = atlas["relPath"]
                .as_str()
                .map(|p| super::asset_path(self.project, p))
                .unwrap_or_else(|| std::path::PathBuf::from("@ldtk/embedded-atlas"));
            let atlas_grid = number(atlas, "tileGridSize", 16.) as i32;
            let manual_tiles = layer["__type"] == "Tiles";
            let tiles = if manual_tiles {
                values(&layer["gridTiles"])
            } else {
                values(&layer["autoLayerTiles"])
            };
            // Four bounds over the signed cell domain take up to 128 projections.
            let visible = (tiles.len() >= 128).then(|| {
                // Tile anchors can be negative or outside the nominal layer dimensions.
                let cell_domain = i64::from(i32::MIN)..i64::from(i32::MAX) + 1;
                let columns = visible_cell_range(
                    cell_domain.clone(),
                    grid,
                    |x| project_axis(0, x),
                    clip.left(),
                    clip.right(),
                );
                let rows = visible_cell_range(
                    cell_domain,
                    grid,
                    |y| project_axis(1, y),
                    clip.top(),
                    clip.bottom(),
                );
                (columns, rows)
            });
            let cell_at = |px: &[Value], axis: usize| {
                (px.get(axis).and_then(Value::as_f64).unwrap_or(0.) as f32 / grid) as i32
            };
            let mut seen = WorldCells::default();
            if let Some((columns, rows)) = &visible {
                done.track_visible(grid as i32, columns.clone(), rows.clone());
                if manual_tiles {
                    seen.track_visible(grid as i32, columns.clone(), rows.clone());
                }
            }
            let mut averages = [None; 256];
            let mut emit = |tile: &Value| {
                let px = values(tile_anchor(tile));
                let x = cell_at(px, 0);
                if visible
                    .as_ref()
                    .is_some_and(|(columns, _)| !columns.contains(&i64::from(x)))
                {
                    return;
                }
                let y = cell_at(px, 1);
                if visible
                    .as_ref()
                    .is_some_and(|(_, rows)| !rows.contains(&i64::from(y)))
                {
                    return;
                }
                let key = (grid as i32, x, y);
                if manual_tiles && !seen.insert(key) {
                    return;
                }
                if done.contains(key) {
                    return;
                }
                let x = x as f32 * grid;
                let y = y as f32 * grid;
                let dst = Rect::from_min_max(
                    egui::pos2(project_axis(0, x), project_axis(1, y)),
                    egui::pos2(project_axis(0, x + grid), project_axis(1, y + grid)),
                );
                if visible.is_none() && !dst.intersects(clip) {
                    return;
                }
                let color = if colors.is_empty() {
                    let src = pair(&tile["src"], [0.; 2]);
                    self.assets.average_tile_color(
                        &atlas_path,
                        [src[0] as i32, src[1] as i32],
                        [atlas_grid; 2],
                    )
                } else {
                    let id = tile["t"].as_u64().unwrap_or(u64::MAX) as usize;
                    let decode = || super::decode_tile_average(colors, id);
                    if let Some(slot) = averages.get_mut(id) {
                        *slot.get_or_insert_with(decode)
                    } else {
                        decode()
                    }
                };
                let Some(color) = color else {
                    return;
                };
                if color[3] < 153 {
                    return;
                }
                done.insert(key);
                painter.rect_filled(dst, 0., Color32::from_rgb(color[0], color[1], color[2]));
                count += 1;
            };
            if manual_tiles {
                tiles.iter().rev().for_each(&mut emit);
            } else {
                tiles.iter().for_each(&mut emit);
            }
        }
        for fd in values(&self.project.json["defs"]["levelFields"]) {
            if fd["editorDisplayMode"] == "LevelTile" {
                let value = super::fields::field_value(level, fd);
                if let Some(icon) = super::fields::value_icon(self.project, fd, &value)
                    && let Some(atlas) = icon["tilesetUid"]
                        .as_i64()
                        .and_then(|uid| self.project.tileset(uid))
                    && let Some(texture) = self.texture(atlas)
                {
                    let source = Rect::from_min_size(
                        [number(&icon, "x", 0.), number(&icon, "y", 0.)].into(),
                        vec2(number(&icon, "w", 16.), number(&icon, "h", 16.)),
                    );
                    let scale = (number(level, "pxWid", 256.) / source.width())
                        .min(number(level, "pxHei", 256.) / source.height());
                    let dst = self.screen([
                        origin[0],
                        origin[1],
                        origin[0] + source.width() * scale,
                        origin[1] + source.height() * scale,
                    ]);
                    painter.image(
                        texture.id(),
                        dst,
                        Rect::from_min_max(
                            (source.min.to_vec2() / texture.size_vec2()).to_pos2(),
                            (source.max.to_vec2() / texture.size_vec2()).to_pos2(),
                        ),
                        Color32::WHITE,
                    );
                    break;
                }
            }
        }
        count
    }
}
fn number_at(v: &Value, index: usize) -> f32 {
    v[index].as_f64().unwrap_or(0.) as f32
}
fn missing_tile(painter: &Painter, rect: Rect) {
    let rect = rect.shrink(2.);
    let line = Stroke::new(3.0_f32, Color32::RED);
    painter.line_segment([rect.left_top(), rect.right_bottom()], line);
    painter.line_segment([rect.right_top(), rect.left_bottom()], line);
}
pub(super) fn preview(
    ctx: &egui::Context,
    painter: &Painter,
    rect: Rect,
    project: &Project,
    def: &Value,
    assets: &mut Assets,
) {
    let w = number(def, "width", 16.);
    let h = number(def, "height", 16.);
    let pivot = [number(def, "pivotX", 0.), number(def, "pivotY", 0.)];
    let entity = json!({"defUid":def["uid"],"width":w,"height":h,"px":[w*pivot[0],h*pivot[1]],"__pivot":pivot,"fieldInstances":values(&def["fieldDefs"]).iter().filter_map(|fd|project.make_field_instance(fd).ok()).collect::<Vec<_>>()});
    let mut view = Viewport {
        center: [w * 0.5, h * 0.5],
        zoom: ((rect.width() - 4.) / w.max(1.))
            .min((rect.height() - 4.) / h.max(1.))
            .max(0.01),
    };
    view.zoom = view.zoom.min(4.);
    let hidden = HashSet::new();
    let key = project.level_keys().first().copied().unwrap_or(LevelKey {
        world: None,
        level: 0,
    });
    let options = RenderOptions {
        selected: key,
        active_layer: 0,
        world: false,
        world_multi_selection: false,
        world_hover: None,
        grid: false,
        details: false,
        single_layer: false,
        hidden: &hidden,
        modified_levels: None,
        render_auto: true,
        tile_tags: false,
        ui_scale: 1.,
        single_layer_intensity: 0.75,
        fields_table: false,
        world_depth: None,
        nearby_dist: 0.,
        parallax: false,
        rulers: false,
        guides: false,
        async_paint: None,
    };
    let mut scene = Scene {
        ctx,
        project,
        view: &view,
        canvas: rect,
        projection_origin: rect.min.into(),
        options: &options,
        assets,
        entity_index: std::cell::OnceCell::new(),
        file_states: Default::default(),
    };
    scene.entity(
        &painter.with_clip_rect(rect),
        &entity,
        &Value::Null,
        Transform {
            origin: [0.; 2],
            scale: 1.,
        },
        1.,
        true,
        key,
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_cell_mask_preserves_previous_coverage_and_keeps_other_keys_distinct() {
        let mut cells = WorldCells::default();
        let marked = [(16, 4, 2), (16, 68, 2), (16, -2, 2), (8, 4, 2)];
        for cell in marked {
            assert!(cells.insert(cell));
        }
        cells.track_visible(16, 4..69, 2..4);
        assert!(cells.visible.is_some());
        for cell in marked {
            assert!(cells.contains(cell));
            assert!(!cells.insert(cell));
        }
        assert!(!cells.contains((16, 4, 3)));
        assert!(cells.insert((16, 4, 3)));
        assert!(!cells.insert((16, 4, 3)));
        assert!(!cells.contains((8, 4, 3)));
        assert!(cells.insert((16, 69, 2)));
        assert!(cells.contains((16, 69, 2)));
    }

    #[test]
    fn pivot_bounds_preserve_instance_dimensions() {
        let entity = json!({"width":40,"height":20,"px":[80,70],"__pivot":[0.5,1]});
        assert_eq!(
            entity_bounds(&entity, &Value::Null),
            Rect::from_min_max([60., 50.].into(), [100., 70.].into())
        );
    }
    #[test]
    fn world_average_cache_is_argb4444() {
        let tileset = json!({"cachedPixelData":{"averageColors":"f1230000"}});
        assert_eq!(
            super::super::cached_tile_average(&tileset, 0),
            Some([17, 34, 51, 255])
        );
        assert_eq!(super::super::cached_tile_average(&tileset, 1), Some([0; 4]));
    }
}
