use crate::{
    assets::Assets,
    theme,
    viewport::{Viewport, tile_uv},
};
use eframe::egui::{self, Color32, FontId, Mesh, Painter, Pos2, Rect, Stroke, StrokeKind};
use ldtk_core::{LevelKey, Project};
use serde_json::Value;
use std::{collections::HashSet, path::PathBuf};

pub fn number(value: &Value, field: &str, fallback: f32) -> f32 {
    value[field].as_f64().map(|v| v as f32).unwrap_or(fallback)
}

pub fn text<'a>(value: &'a Value, field: &str, fallback: &'a str) -> &'a str {
    value[field].as_str().unwrap_or(fallback)
}

pub fn placements(project: &Project, selected: LevelKey) -> Vec<(LevelKey, [f32; 4])> {
    let settings = selected
        .world
        .and_then(|i| project.json["worlds"].get(i))
        .unwrap_or(&project.json);
    let layout = text(settings, "worldLayout", "Free");
    let mut cursor = [0.0_f32; 2];
    project
        .level_keys()
        .into_iter()
        .filter(|key| key.world == selected.world)
        .filter_map(|key| {
            let level = project.level(key)?;
            let size = [number(level, "pxWid", 256.0), number(level, "pxHei", 256.0)];
            let origin = match layout {
                "LinearHorizontal" => {
                    let pos = cursor;
                    cursor[0] += size[0] + 48.0;
                    pos
                }
                "LinearVertical" => {
                    let pos = cursor;
                    cursor[1] += size[1] + 48.0;
                    pos
                }
                _ => [number(level, "worldX", 0.0), number(level, "worldY", 0.0)],
            };
            Some((
                key,
                [
                    origin[0],
                    origin[1],
                    origin[0] + size[0],
                    origin[1] + size[1],
                ],
            ))
        })
        .collect()
}

pub fn bounds(project: &Project, selected: LevelKey, world: bool) -> [f32; 4] {
    if !world {
        return project
            .level(selected)
            .map(|l| {
                [
                    0.0,
                    0.0,
                    number(l, "pxWid", 256.0),
                    number(l, "pxHei", 256.0),
                ]
            })
            .unwrap_or([0.0, 0.0, 256.0, 256.0]);
    }
    placements(project, selected).iter().fold(
        [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ],
        |a, (_, b)| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        },
    )
}

pub fn asset_path(project: &Project, relative: &str) -> PathBuf {
    project
        .path
        .as_deref()
        .and_then(|p| p.parent())
        .unwrap_or(std::path::Path::new("."))
        .join(relative)
}

fn screen_rect(view: &Viewport, canvas: Rect, bounds: [f32; 4]) -> Rect {
    let a = view.to_screen(
        [bounds[0], bounds[1]],
        canvas.min.into(),
        canvas.size().into(),
    );
    let b = view.to_screen(
        [bounds[2], bounds[3]],
        canvas.min.into(),
        canvas.size().into(),
    );
    Rect::from_min_max(a.into(), b.into())
}

pub struct RenderOptions<'a> {
    pub selected: LevelKey,
    pub active_layer: usize,
    pub world: bool,
    pub grid: bool,
    pub details: bool,
    pub single_layer: bool,
    pub hidden: &'a HashSet<i64>,
}

pub fn draw(
    ctx: &egui::Context,
    painter: &Painter,
    canvas: Rect,
    project: &Project,
    view: &Viewport,
    options: &RenderOptions<'_>,
    assets: &mut Assets,
) -> usize {
    painter.rect_filled(canvas, 0.0, theme::DARK);
    let levels = if options.world {
        placements(project, options.selected)
    } else {
        vec![(options.selected, bounds(project, options.selected, false))]
    };
    let mut visible_tiles = 0;
    for (key, bounds) in levels {
        let rect = screen_rect(view, canvas, bounds);
        if !rect.intersects(canvas) {
            continue;
        }
        let Some(level) = project.level(key) else {
            continue;
        };
        let origin = [bounds[0], bounds[1]];
        painter.rect_filled(rect, 0.0, theme::color(text(level, "__bgColor", "#696A79")));
        let clipped = painter.with_clip_rect(canvas.intersect(rect));
        draw_background(ctx, &clipped, canvas, project, level, view, origin, assets);
        if let Some(layers) = level["layerInstances"].as_array() {
            for (index, layer) in layers.iter().enumerate().rev() {
                let uid = layer["layerDefUid"].as_i64().unwrap_or(-1);
                if options.hidden.contains(&uid) || layer["visible"].as_bool() == Some(false) {
                    continue;
                }
                let alpha = number(layer, "__opacity", 1.0)
                    * if options.single_layer && index != options.active_layer {
                        0.15
                    } else {
                        1.0
                    };
                let offset = [
                    origin[0] + number(layer, "__pxTotalOffsetX", 0.0),
                    origin[1] + number(layer, "__pxTotalOffsetY", 0.0),
                ];
                let grid = number(layer, "__gridSize", 16.0).max(1.0);
                let layer_type = text(layer, "__type", "");
                if layer_type == "IntGrid"
                    && options.details
                    && layer["autoLayerTiles"]
                        .as_array()
                        .is_none_or(|a| a.is_empty())
                    && let (Some(cells), Some(def)) =
                        (layer["intGridCsv"].as_array(), project.layer_def(uid))
                {
                    let width = number(layer, "__cWid", 1.0).max(1.0) as usize;
                    let viewport_min =
                        view.to_world(canvas.min.into(), canvas.min.into(), canvas.size().into());
                    let viewport_max =
                        view.to_world(canvas.max.into(), canvas.min.into(), canvas.size().into());
                    let x0 =
                        (((viewport_min[0] - offset[0]) / grid).floor() as i32).max(0) as usize;
                    let y0 =
                        (((viewport_min[1] - offset[1]) / grid).floor() as i32).max(0) as usize;
                    let x1 = (((viewport_max[0] - offset[0]) / grid).ceil() as i32)
                        .max(0)
                        .min(width as i32) as usize;
                    let height = cells.len().div_ceil(width);
                    let y1 = (((viewport_max[1] - offset[1]) / grid).ceil() as i32)
                        .max(0)
                        .min(height as i32) as usize;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let value = cells
                                .get(x + y * width)
                                .and_then(Value::as_i64)
                                .unwrap_or(0);
                            if value == 0 {
                                continue;
                            }
                            let color = def["intGridValues"]
                                .as_array()
                                .and_then(|vs| {
                                    vs.iter().find(|v| v["value"].as_i64() == Some(value))
                                })
                                .map(|v| theme::color(text(v, "color", "#ffcc00")))
                                .unwrap_or(theme::ACCENT);
                            let cell = screen_rect(
                                view,
                                canvas,
                                [
                                    offset[0] + x as f32 * grid,
                                    offset[1] + y as f32 * grid,
                                    offset[0] + (x + 1) as f32 * grid,
                                    offset[1] + (y + 1) as f32 * grid,
                                ],
                            );
                            clipped.rect_filled(cell, 0.0, color.gamma_multiply(alpha));
                            visible_tiles += 1;
                        }
                    }
                }
                if let Some(uid) = layer["__tilesetDefUid"].as_i64()
                    && let Some(tileset) = project.tileset(uid)
                    && let Some(relative) = tileset["relPath"].as_str()
                    && let Some(texture) = assets.texture(ctx, &asset_path(project, relative))
                {
                    let mut mesh = Mesh::with_texture(texture.id());
                    let tile_size = number(tileset, "tileGridSize", grid);
                    for field in ["autoLayerTiles", "gridTiles"] {
                        if let Some(tiles) = layer[field].as_array() {
                            for tile in tiles {
                                let pos = [
                                    offset[0] + tile["px"][0].as_f64().unwrap_or(0.0) as f32,
                                    offset[1] + tile["px"][1].as_f64().unwrap_or(0.0) as f32,
                                ];
                                let rect = screen_rect(
                                    view,
                                    canvas,
                                    [pos[0], pos[1], pos[0] + tile_size, pos[1] + tile_size],
                                );
                                if !rect.intersects(clipped.clip_rect()) {
                                    continue;
                                }
                                let uv = tile_uv(
                                    [
                                        tile["src"][0].as_f64().unwrap_or(0.0) as f32,
                                        tile["src"][1].as_f64().unwrap_or(0.0) as f32,
                                    ],
                                    [tile_size, tile_size],
                                    texture.size_vec2().into(),
                                    tile["f"].as_i64().unwrap_or(0),
                                );
                                mesh.add_rect_with_uv(
                                    rect,
                                    Rect {
                                        min: uv[0].into(),
                                        max: uv[1].into(),
                                    },
                                    Color32::WHITE.gamma_multiply(alpha * number(tile, "a", 1.0)),
                                );
                                visible_tiles += 1;
                            }
                        }
                    }
                    clipped.add(egui::Shape::mesh(mesh));
                }
                if layer_type == "Entities"
                    && let Some(entities) = layer["entityInstances"].as_array()
                {
                    for entity in entities {
                        draw_entity(
                            ctx,
                            painter,
                            canvas,
                            project,
                            entity,
                            view,
                            offset,
                            alpha,
                            options.details,
                            assets,
                        );
                    }
                }
            }
            if options.grid && !options.world {
                let grid = layers
                    .get(options.active_layer)
                    .map(|l| number(l, "__gridSize", 16.0))
                    .unwrap_or(16.0)
                    .max(1.0);
                if grid * view.zoom >= 6.0 {
                    let offset = layers
                        .get(options.active_layer)
                        .map(|l| {
                            [
                                number(l, "__pxTotalOffsetX", 0.0),
                                number(l, "__pxTotalOffsetY", 0.0),
                            ]
                        })
                        .unwrap_or([0.0; 2]);
                    let start =
                        view.to_world(canvas.min.into(), canvas.min.into(), canvas.size().into());
                    let end =
                        view.to_world(canvas.max.into(), canvas.min.into(), canvas.size().into());
                    let x0 = ((start[0] - offset[0]) / grid).floor() as i32;
                    let x1 = ((end[0] - offset[0]) / grid).ceil() as i32;
                    let y0 = ((start[1] - offset[1]) / grid).floor() as i32;
                    let y1 = ((end[1] - offset[1]) / grid).ceil() as i32;
                    let line = Stroke::new(1.0_f32, Color32::from_white_alpha(20));
                    for x in x0..=x1 {
                        let sx = view.to_screen(
                            [offset[0] + x as f32 * grid, 0.0],
                            canvas.min.into(),
                            canvas.size().into(),
                        )[0];
                        clipped.line_segment(
                            [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
                            line,
                        );
                    }
                    for y in y0..=y1 {
                        let sy = view.to_screen(
                            [0.0, offset[1] + y as f32 * grid],
                            canvas.min.into(),
                            canvas.size().into(),
                        )[1];
                        clipped.line_segment(
                            [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
                            line,
                        );
                    }
                }
            }
        }
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(
                if key == options.selected {
                    2.0_f32
                } else {
                    1.0_f32
                },
                if key == options.selected {
                    theme::ACCENT
                } else {
                    theme::LIGHT
                },
            ),
            StrokeKind::Inside,
        );
        if options.world {
            painter.text(
                rect.left_top() - egui::vec2(0.0, 5.0),
                egui::Align2::LEFT_BOTTOM,
                text(level, "identifier", "Level"),
                FontId::proportional(15.0),
                theme::TEXT,
            );
        }
    }
    visible_tiles
}

#[allow(clippy::too_many_arguments)]
fn draw_background(
    ctx: &egui::Context,
    painter: &Painter,
    canvas: Rect,
    project: &Project,
    level: &Value,
    view: &Viewport,
    origin: [f32; 2],
    assets: &mut Assets,
) {
    let Some(relative) = level["bgRelPath"].as_str() else {
        return;
    };
    let Some(texture) = assets.texture(ctx, &asset_path(project, relative)) else {
        return;
    };
    let data = &level["__bgPos"];
    let size = texture.size_vec2();
    let top = [
        origin[0] + data["topLeftPx"][0].as_f64().unwrap_or(0.0) as f32,
        origin[1] + data["topLeftPx"][1].as_f64().unwrap_or(0.0) as f32,
    ];
    let crop = [
        data["cropRect"][0].as_f64().unwrap_or(0.0) as f32,
        data["cropRect"][1].as_f64().unwrap_or(0.0) as f32,
        data["cropRect"][2].as_f64().unwrap_or(size.x as f64) as f32,
        data["cropRect"][3].as_f64().unwrap_or(size.y as f64) as f32,
    ];
    let scale = [
        data["scale"][0].as_f64().unwrap_or(1.0) as f32,
        data["scale"][1].as_f64().unwrap_or(1.0) as f32,
    ];
    let rect = screen_rect(
        view,
        canvas,
        [
            top[0],
            top[1],
            top[0] + crop[2] * scale[0],
            top[1] + crop[3] * scale[1],
        ],
    );
    let uv = Rect::from_min_max(
        egui::pos2(crop[0] / size.x, crop[1] / size.y),
        egui::pos2((crop[0] + crop[2]) / size.x, (crop[1] + crop[3]) / size.y),
    );
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

#[allow(clippy::too_many_arguments)]
fn draw_entity(
    ctx: &egui::Context,
    painter: &Painter,
    canvas: Rect,
    project: &Project,
    entity: &Value,
    view: &Viewport,
    offset: [f32; 2],
    alpha: f32,
    details: bool,
    assets: &mut Assets,
) {
    let width = number(entity, "width", 16.0);
    let height = number(entity, "height", 16.0);
    let pos = [
        offset[0] + entity["px"][0].as_f64().unwrap_or(0.0) as f32
            - width * entity["__pivot"][0].as_f64().unwrap_or(0.0) as f32,
        offset[1] + entity["px"][1].as_f64().unwrap_or(0.0) as f32
            - height * entity["__pivot"][1].as_f64().unwrap_or(0.0) as f32,
    ];
    let rect = screen_rect(
        view,
        canvas,
        [pos[0], pos[1], pos[0] + width, pos[1] + height],
    );
    if !rect.intersects(canvas) {
        return;
    }
    let color = theme::color(text(entity, "__smartColor", "#ffcc00")).gamma_multiply(alpha);
    let mut has_tile = false;
    if let Some(uid) = entity["__tile"]["tilesetUid"].as_i64()
        && let Some(tileset) = project.tileset(uid)
        && let Some(relative) = tileset["relPath"].as_str()
        && let Some(texture) = assets.texture(ctx, &asset_path(project, relative))
    {
        let tile = &entity["__tile"];
        let uv = tile_uv(
            [number(tile, "x", 0.0), number(tile, "y", 0.0)],
            [number(tile, "w", width), number(tile, "h", height)],
            texture.size_vec2().into(),
            0,
        );
        painter.image(
            texture.id(),
            rect,
            Rect {
                min: uv[0].into(),
                max: uv[1].into(),
            },
            Color32::WHITE.gamma_multiply(alpha),
        );
        has_tile = true;
    }
    if !has_tile {
        painter.rect_filled(rect, 0.0, color.gamma_multiply(0.35));
    }
    if details {
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, color), StrokeKind::Inside);
        if view.zoom >= 1.5 {
            painter.text(
                rect.center_top() - egui::vec2(0.0, 3.0),
                egui::Align2::CENTER_BOTTOM,
                text(entity, "__identifier", "Entity"),
                FontId::proportional(12.0),
                color,
            );
        }
    }
}
