use super::{RenderOptions, Scene, Transform, geometry, lighten, number, pair, text, values};
use crate::{assets::Assets, theme, viewport::Viewport};
use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Stroke, StrokeKind, vec2};
use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
use std::collections::HashSet;

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
impl Scene<'_> {
    pub(super) fn smart_tile(&self, entity: &Value, def: &Value) -> Option<Value> {
        for fd in values(&def["fieldDefs"]) {
            if fd["editorDisplayMode"] == "EntityTile" {
                let value = super::fields::field_value(entity, fd);
                let first = if fd["isArray"] == true {
                    &value[0]
                } else {
                    &value
                };
                if let Some(tile) = super::fields::value_icon(self.project, fd, first) {
                    return Some(tile);
                }
            }
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
        if !rect
            .expand(300. * self.options.ui_scale)
            .intersects(self.canvas)
            && !values(&def["fieldDefs"]).iter().any(|f| {
                matches!(
                    text(f, "editorDisplayMode", ""),
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
            if tile.is_some() || mode == "Rectangle" {
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
        if let Some(tile) = tile {
            if let Some(atlas) = tile["tilesetUid"]
                .as_i64()
                .and_then(|uid| self.project.tileset(uid))
            {
                if let Some(texture) = self.texture(atlas) {
                    let source = Rect::from_min_size(
                        [number(&tile, "x", 0.), number(&tile, "y", 0.)].into(),
                        vec2(number(&tile, "w", 16.), number(&tile, "h", 16.)),
                    );
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
                            Color32::WHITE
                                .gamma_multiply(core_alpha * number(def, "tileOpacity", 1.)),
                        );
                    }
                    painter.add(egui::Shape::mesh(mesh));
                } else if visible {
                    missing_tile(painter, rect);
                }
            } else if visible {
                missing_tile(painter, rect);
            }
        } else if mode == "Tile" && visible {
            missing_tile(painter, rect);
        }
        if visible {
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
        let mut done = HashSet::new();
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
                    let px = pair(&tile["px"], [0.; 2]);
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
            if layer["__type"] == "IntGrid" && layer["__tilesetDefUid"].is_null() {
                let width = number(layer, "__cWid", 0.) as usize;
                if width == 0 {
                    continue;
                }
                for (index, value) in values(&layer["intGridCsv"]).iter().enumerate() {
                    let value = value.as_i64().unwrap_or(0);
                    if value == 0 {
                        continue;
                    }
                    let cell = [(index % width) as i32, (index / width) as i32];
                    let key = (grid as i32, cell[0], cell[1]);
                    if done.contains(&key) {
                        continue;
                    }
                    let color = values(&def["intGridValues"])
                        .iter()
                        .find(|d| d["value"].as_i64() == Some(value))
                        .map(|d| theme::color(text(d, "color", "#ffcc00")))
                        .unwrap_or(theme::ACCENT);
                    let dst = self.screen(transform.rect(Rect::from_min_size(
                        [cell[0] as f32 * grid, cell[1] as f32 * grid].into(),
                        vec2(grid, grid),
                    )));
                    if dst.intersects(painter.clip_rect()) {
                        done.insert(key);
                        painter.rect_filled(dst, 0., color);
                        count += 1;
                    }
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
            let tiles = if layer["__type"] == "Tiles" {
                values(&layer["gridTiles"])
            } else {
                values(&layer["autoLayerTiles"])
            };
            let mut emit = |tile: &Value| {
                let px = pair(&tile["px"], [0.; 2]);
                let cell = [(px[0] / grid) as i32, (px[1] / grid) as i32];
                let key = (grid as i32, cell[0], cell[1]);
                if done.contains(&key) {
                    return;
                }
                let id = tile["t"].as_u64().unwrap_or(u64::MAX) as usize;
                let Some(start) = id.checked_mul(4) else {
                    return;
                };
                let color = if colors.is_empty() {
                    let src = pair(&tile["src"], [0.; 2]);
                    self.assets.average_tile_color(
                        &atlas_path,
                        [src[0] as i32, src[1] as i32],
                        [atlas_grid; 2],
                    )
                } else {
                    colors.get(start..start + 4).and_then(average_color)
                };
                let Some(color) = color else {
                    return;
                };
                if color[3] < 153 {
                    return;
                }
                let dst = self.screen(transform.rect(Rect::from_min_size(
                    [cell[0] as f32 * grid, cell[1] as f32 * grid].into(),
                    vec2(grid, grid),
                )));
                if !dst.intersects(painter.clip_rect()) {
                    return;
                }
                done.insert(key);
                painter.rect_filled(dst, 0., Color32::from_rgb(color[0], color[1], color[2]));
                count += 1;
            };
            if layer["__type"] == "Tiles" {
                let mut seen = HashSet::new();
                for tile in tiles.iter().rev() {
                    let px = pair(&tile["px"], [0.; 2]);
                    if seen.insert(((px[0] / grid) as i32, (px[1] / grid) as i32)) {
                        emit(tile);
                    }
                }
            } else {
                for tile in tiles {
                    emit(tile)
                }
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
fn average_color(s: &str) -> Option<[u8; 4]> {
    let n = u16::from_str_radix(s, 16).ok()?;
    Some([
        ((n >> 8) & 15) as u8 * 17,
        ((n >> 4) & 15) as u8 * 17,
        (n & 15) as u8 * 17,
        ((n >> 12) & 15) as u8 * 17,
    ])
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
        grid: false,
        details: false,
        single_layer: false,
        hidden: &hidden,
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
    };
    let mut scene = Scene {
        ctx,
        project,
        view: &view,
        canvas: rect,
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
    fn pivot_bounds_preserve_instance_dimensions() {
        let entity = json!({"width":40,"height":20,"px":[80,70],"__pivot":[0.5,1]});
        assert_eq!(
            entity_bounds(&entity, &Value::Null),
            Rect::from_min_max([60., 50.].into(), [100., 70.].into())
        );
    }
    #[test]
    fn world_average_cache_is_argb4444() {
        assert_eq!(average_color("f123"), Some([17, 34, 51, 255]));
        assert_eq!(average_color("0000"), Some([0; 4]));
    }
}
