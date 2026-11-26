#[path = "render_blur.rs"]
mod blur;
pub fn configure_gpu(state: Option<&eframe::egui_wgpu::RenderState>) {
    blur::configure(state);
}
pub fn submit_pending_gpu_uploads() {
    if let Some(state) = blur::gpu() {
        state.queue.submit(std::iter::empty());
    }
}
pub fn begin_picker_filter(painter: &egui::Painter, active: bool) -> Option<usize> {
    if active {
        Some(blur::begin(painter))
    } else {
        blur::release_picker();
        None
    }
}
pub fn finish_picker_filter(painter: &egui::Painter, start: Option<usize>, rect: egui::Rect) {
    if let Some(start) = start {
        blur::finish_picker(painter, start, rect);
    }
}
#[path = "bitmap_font.rs"]
pub(crate) mod bitmap_font;
#[path = "render_entities.rs"]
mod entities;
#[path = "render_fields.rs"]
mod fields;
#[path = "render_geometry.rs"]
mod geometry;
#[path = "render_ghost.rs"]
mod ghost;
#[path = "render_outline.rs"]
mod outline;
#[path = "render_shadow.rs"]
mod shadow;
use crate::{
    assets::Assets,
    theme,
    viewport::{Viewport, tile_uv},
};
use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Stroke, StrokeKind};
pub(crate) use entities::{entity_field_tile, entity_smart_color};
pub(crate) use fields::{draw_entity_ref_link, level_smart_color};
use ldtk_core::{LevelKey, Project};
use serde_json::Value;
use std::{collections::HashSet, path::PathBuf};

pub(crate) fn entity_display_order(layer: &Value, def: &Value) -> Vec<usize> {
    entity_order(values(&layer["entityInstances"]), def)
}

fn entity_order(entities: &[Value], def: &Value) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entities.len()).collect();
    let y = |&i: &usize| entities[i]["px"][1].as_i64().unwrap_or(0);
    match def["entityDepthSorting"].as_str() {
        Some("YAscending") => order.sort_by_cached_key(y),
        Some("YDescending") => order.sort_by_cached_key(|i| std::cmp::Reverse(y(i))),
        _ => {}
    }
    order
}

fn guide_grid_color(def: &Value, base: Color32) -> Color32 {
    match def["guideColor"].as_str() {
        Some(color) => {
            theme::color(color).gamma_multiply(number(def, "guideOpacity", 0.33).clamp(0., 1.))
        }
        None => base.gamma_multiply(0.33),
    }
}

pub fn number(value: &Value, field: &str, fallback: f32) -> f32 {
    value[field].as_f64().map(|v| v as f32).unwrap_or(fallback)
}

pub fn text<'a>(value: &'a Value, field: &str, fallback: &'a str) -> &'a str {
    value[field].as_str().unwrap_or(fallback)
}

pub(crate) fn layer_visible_in_tiles_only(kind: &str, def: &Value) -> bool {
    matches!(kind, "Tiles" | "AutoLayer") || kind == "IntGrid" && !def["tilesetDefUid"].is_null()
}

pub fn placements(project: &Project, selected: LevelKey) -> Vec<(LevelKey, [f32; 4])> {
    iter_placements(project, selected)
        .map(|(key, _, bounds)| (key, bounds))
        .collect()
}

pub(crate) fn world_layout(project: &Project, selected: LevelKey) -> &str {
    let settings = selected
        .world
        .and_then(|i| project.json["worlds"].get(i))
        .unwrap_or(&project.json);
    text(settings, "worldLayout", "Free")
}

fn placement_bounds(origin: [f32; 2], size: [f32; 2]) -> [f32; 4] {
    [
        origin[0],
        origin[1],
        origin[0] + size[0],
        origin[1] + size[1],
    ]
}

pub(crate) fn free_level_bounds(level: &Value) -> [f32; 4] {
    placement_bounds(
        [number(level, "worldX", 0.), number(level, "worldY", 0.)],
        [number(level, "pxWid", 256.), number(level, "pxHei", 256.)],
    )
}

pub(crate) fn iter_placements(
    project: &Project,
    selected: LevelKey,
) -> impl Iterator<Item = (LevelKey, &Value, [f32; 4])> {
    let layout = world_layout(project, selected);
    let mut cursor = [0.0_f32; 2];
    project
        .levels_in_world(selected.world)
        .map(move |(key, level)| {
            let axis = match layout {
                "LinearHorizontal" => 0,
                "LinearVertical" => 1,
                _ => return (key, level, free_level_bounds(level)),
            };
            let size = [number(level, "pxWid", 256.), number(level, "pxHei", 256.)];
            let bounds = placement_bounds(cursor, size);
            cursor[axis] += size[axis] + 48.0;
            (key, level, bounds)
        })
}

const EMPTY_BOUNDS: [f32; 4] = [
    f32::INFINITY,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::NEG_INFINITY,
];

fn union_bounds(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
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
    iter_placements(project, selected).fold(EMPTY_BOUNDS, |a, (_, _, b)| union_bounds(a, b))
}

pub fn asset_path(project: &Project, relative: &str) -> PathBuf {
    project
        .path
        .as_deref()
        .and_then(|p| p.parent())
        .unwrap_or(std::path::Path::new("."))
        .join(relative)
}

pub(crate) struct AsyncPaintPreview {
    pub level: LevelKey,
    pub deadline: f64,
    pub layers: std::collections::HashMap<i64, AsyncLayerPreview>,
    pub hint_grid: Option<f32>,
    pub hints: std::collections::BTreeMap<[i32; 2], Color32>,
}

#[derive(Default)]
pub(crate) struct AsyncLayerPreview {
    pub old_intgrid: std::collections::BTreeMap<[i32; 2], i64>,
    pub old_intgrid_cells: Option<Vec<Value>>,
    pub old_tiles: Option<Vec<Value>>,
    pub old_entities: Option<Vec<Value>>,
}

impl AsyncPaintPreview {
    pub fn is_active_for(&self, level: LevelKey, now: f64) -> bool {
        self.level == level && now < self.deadline
    }
}

pub(crate) fn cached_tile_average(tileset: &Value, tile_id: usize) -> Option<[u8; 4]> {
    let colors = tileset["cachedPixelData"]["averageColors"].as_str()?;
    decode_tile_average(colors, tile_id)
}

fn decode_tile_average(colors: &str, tile_id: usize) -> Option<[u8; 4]> {
    let start = tile_id.checked_mul(4)?;
    let n = u16::from_str_radix(colors.get(start..start.checked_add(4)?)?, 16).ok()?;
    Some([
        ((n >> 8) & 15) as u8 * 17,
        ((n >> 4) & 15) as u8 * 17,
        (n & 15) as u8 * 17,
        ((n >> 12) & 15) as u8 * 17,
    ])
}

#[derive(Clone)]
pub struct RenderOptions<'a> {
    pub selected: LevelKey,
    pub active_layer: usize,
    pub world: bool,
    pub world_multi_selection: bool,
    pub world_hover: Option<LevelKey>,
    pub grid: bool,
    pub details: bool,
    pub single_layer: bool,
    pub hidden: &'a HashSet<i64>,
    pub modified_levels: Option<&'a std::collections::BTreeSet<String>>,
    pub render_auto: bool,
    pub tile_tags: bool,
    pub ui_scale: f32,
    pub single_layer_intensity: f32,
    pub fields_table: bool,
    pub world_depth: Option<i64>,
    pub nearby_dist: f32,
    pub parallax: bool,
    pub rulers: bool,
    pub guides: bool,
    pub(crate) async_paint: Option<&'a AsyncPaintPreview>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Transform {
    pub origin: [f32; 2],
    pub scale: f32,
}
impl Transform {
    pub fn point(self, p: [f32; 2]) -> [f32; 2] {
        [
            self.origin[0] + p[0] * self.scale,
            self.origin[1] + p[1] * self.scale,
        ]
    }
    pub fn rect(self, r: Rect) -> [f32; 4] {
        let a = self.point(r.min.into());
        let b = self.point(r.max.into());
        [a[0], a[1], b[0], b[1]]
    }
    pub fn offset(self, p: [f32; 2]) -> Self {
        Self {
            origin: self.point(p),
            scale: self.scale,
        }
    }
}
pub(super) fn values(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

pub(super) fn tile_anchor(tile: &Value) -> &Value {
    // LDtk writes the anchor first in both manual and generated tile records.
    if let Some((key, anchor)) = tile.as_object().and_then(|fields| fields.iter().next())
        && key == "px"
    {
        return anchor;
    }
    &tile["px"]
}

// A brush preview may touch only one color even on a layer with a large palette.
fn intgrid_cell_colors(def: &Value, alpha: f32) -> impl FnMut(i64) -> Color32 + '_ {
    let palette = values(&def["intGridValues"]);
    let resolve = move |value| {
        palette
            .iter()
            .find(|entry| entry["value"].as_i64() == Some(value))
            .map(|entry| theme::color(text(entry, "color", "#ffcc00")))
            .unwrap_or(theme::ACCENT)
            .gamma_multiply(alpha)
    };
    let mut common = [None; 256];
    let mut other = std::collections::HashMap::new();
    move |value| {
        if (0..common.len() as i64).contains(&value) {
            *common[value as usize].get_or_insert_with(|| resolve(value))
        } else {
            *other.entry(value).or_insert_with(|| resolve(value))
        }
    }
}

pub(super) fn pair(value: &Value, fallback: [f32; 2]) -> [f32; 2] {
    let pair = values(value);
    [
        pair.first()
            .and_then(Value::as_f64)
            .map_or(fallback[0], |v| v as f32),
        pair.get(1)
            .and_then(Value::as_f64)
            .map_or(fallback[1], |v| v as f32),
    ]
}
pub(super) fn lighten(c: Color32, ratio: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r() as f32 + (255 - c.r()) as f32 * ratio) as u8,
        (c.g() as f32 + (255 - c.g()) as f32 * ratio) as u8,
        (c.b() as f32 + (255 - c.b()) as f32 * ratio) as u8,
        c.a(),
    )
}
pub(super) fn darken(c: Color32, ratio: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r() as f32 * (1. - ratio)) as u8,
        (c.g() as f32 * (1. - ratio)) as u8,
        (c.b() as f32 * (1. - ratio)) as u8,
        c.a(),
    )
}
fn layer_pixel_size(level: &Value, layer: &Value) -> [i64; 2] {
    [("pxWid", "pxOffsetX"), ("pxHei", "pxOffsetY")].map(|(size, offset)| {
        level[size]
            .as_i64()
            .unwrap_or(0)
            .saturating_sub(layer[offset].as_i64().unwrap_or(0))
    })
}

fn layer_transform_for(
    level: &Value,
    layer: &Value,
    def: &Value,
    origin: [f32; 2],
    view: &Viewport,
    parallax: bool,
) -> Transform {
    let factor = [
        number(def, "parallaxFactorX", 0.),
        number(def, "parallaxFactorY", 0.),
    ];
    let scaling = def["parallaxScaling"].as_bool().unwrap_or(false);
    let scale = if scaling && parallax {
        (1. - factor[0]).max(0.01)
    } else {
        1.
    };
    let mut offset = origin;
    if parallax {
        for axis in 0..2 {
            let half = if scaling {
                0.
            } else {
                (number(level, if axis == 0 { "pxWid" } else { "pxHei" }, 0.)
                    - number(layer, if axis == 0 { "pxOffsetX" } else { "pxOffsetY" }, 0.))
                    * 0.5
            };
            offset[axis] += (view.center[axis] - origin[axis] - half) * factor[axis];
        }
    }
    Transform {
        origin: offset,
        scale,
    }
}
pub(super) fn editing_transform(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
) -> Option<Transform> {
    let level = project.level(key)?;
    let layer = project.layer(key, index)?;
    let def = project.layer_def(layer["layerDefUid"].as_i64()?)?;
    Some(
        layer_transform_for(level, layer, def, [0.; 2], view, true).offset([
            number(layer, "__pxTotalOffsetX", 0.),
            number(layer, "__pxTotalOffsetY", 0.),
        ]),
    )
}
pub fn world_to_layer(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
    world: [f32; 2],
) -> [f32; 2] {
    editing_transform(project, key, index, view).map_or(world, |t| {
        [
            (world[0] - t.origin[0]) / t.scale,
            (world[1] - t.origin[1]) / t.scale,
        ]
    })
}
pub(super) fn pointer_transform(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
) -> Option<Transform> {
    let level = project.level(key)?;
    let layer = project.layer(key, index)?;
    let def = project.layer_def(layer["layerDefUid"].as_i64()?)?;
    let mut transform = layer_transform_for(level, layer, def, [0.; 2], view, true);
    let offset = [
        number(layer, "__pxTotalOffsetX", 0.),
        number(layer, "__pxTotalOffsetY", 0.),
    ];
    // Coords adds the raw layer offset before rounding, independently of scale.
    transform.origin = std::array::from_fn(|axis| (transform.origin[axis] + offset[axis]).round());
    Some(transform)
}

pub fn pointer_to_layer(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
    level_pixel: [f32; 2],
) -> [f32; 2] {
    // Coords uses the layer's rounded parallax origin and then truncates
    // after undoing its scale. Drawing uses the unsnapped translation.
    pointer_transform(project, key, index, view).map_or(level_pixel, |t| {
        std::array::from_fn(|axis| ((level_pixel[axis] - t.origin[axis]) / t.scale).trunc())
    })
}

pub fn layer_to_pointer_world(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
    local: [f32; 2],
) -> [f32; 2] {
    pointer_transform(project, key, index, view).map_or(local, |t| t.point(local))
}

pub fn layer_to_world(
    project: &Project,
    key: LevelKey,
    index: usize,
    view: &Viewport,
    local: [f32; 2],
) -> [f32; 2] {
    editing_transform(project, key, index, view).map_or(local, |t| t.point(local))
}
pub(super) struct Scene<'a> {
    pub ctx: &'a egui::Context,
    pub project: &'a Project,
    pub view: &'a Viewport,
    pub canvas: Rect,
    projection_origin: [f32; 2],
    pub options: &'a RenderOptions<'a>,
    pub assets: &'a mut Assets,
    entity_index: fields::EntityIndex<'a>,
    file_states: std::cell::RefCell<std::collections::HashMap<PathBuf, bool>>,
}
impl Scene<'_> {
    fn world_background(&mut self, painter: &Painter) {
        let base = theme::color(text(&self.project.json, "bgColor", "#292c39"));
        let target = [129., 135., 189.];
        let delta = -(0.036 / (self.view.zoom * self.ctx.pixels_per_point())).clamp(0., 1.) * 255.;
        let channels = [base.r(), base.g(), base.b()].map(|v| v as f32);
        let c = Color32::from_rgb(
            (channels[0] * 0.15 + target[0] * 0.85 + delta).clamp(0., 255.) as u8,
            (channels[1] * 0.15 + target[1] * 0.85 + delta).clamp(0., 255.) as u8,
            (channels[2] * 0.15 + target[2] * 0.85 + delta).clamp(0., 255.) as u8,
        );
        painter.rect_filled(self.canvas, 0., c.gamma_multiply(0.3));
        if let Some(texture) = self.assets.builtin(
            self.ctx,
            "world-large-stripes",
            include_bytes!("../assets/img/world-large-stripes.png"),
        ) {
            let size = texture.size_vec2() / self.ctx.pixels_per_point();
            let mut mesh = Mesh::with_texture(texture.id());
            let mut y = self.canvas.top();
            while y < self.canvas.bottom() {
                let mut x = self.canvas.left();
                while x < self.canvas.right() {
                    let dst = Rect::from_min_size(egui::pos2(x, y), size).intersect(self.canvas);
                    let uv = Rect::from_min_size(Pos2::ZERO, dst.size() / size);
                    mesh.add_rect_with_uv(dst, uv, Color32::WHITE.gamma_multiply(0.15));
                    x += size.x;
                }
                y += size.y;
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
    fn world_title(&mut self, painter: &Painter, b: [f32; 4]) {
        if !self.options.details {
            return;
        }
        if !b.iter().all(|v| v.is_finite()) {
            return;
        }
        let world = self
            .options
            .selected
            .world
            .and_then(|i| self.project.json["worlds"].get(i))
            .unwrap_or(&self.project.json);
        let multiple = values(&self.project.json["flags"])
            .iter()
            .any(|v| v == "MultiWorlds");
        let file = self
            .project
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("World");
        let label = if multiple {
            text(world, "identifier", file)
        } else {
            file
        };
        let size = bitmap_font::title_measure(label);
        let scale = self.view.zoom * (8f32).min((b[2] - b[0]) / size.x.max(1.) * 2.);
        let anchor = self.point([(b[0] + b[2]) * 0.5, b[1]])
            - egui::vec2(
                size.x * scale * 0.5,
                size.y * scale + 64. / self.ctx.pixels_per_point(),
            );
        bitmap_font::title_paint(
            self.ctx,
            painter,
            self.assets,
            anchor,
            label,
            lighten(
                theme::color(text(&self.project.json, "bgColor", "#292c39")),
                0.3,
            ),
            scale,
        );
    }
    fn world_grid(&self, painter: &Painter) {
        let settings = self
            .options
            .selected
            .world
            .and_then(|i| self.project.json["worlds"].get(i))
            .unwrap_or(&self.project.json);
        let layout = text(settings, "worldLayout", "Free");
        let zoom = self.view.zoom;
        let (spacing, alpha) = if layout == "GridVania" && zoom >= 0.1 && self.options.details {
            (
                [
                    number(settings, "worldGridWidth", 256.),
                    number(settings, "worldGridHeight", 256.),
                ],
                0.1 + 0.2 * ((zoom - 0.1) / 0.3).min(1.),
            )
        } else if layout == "Free" && zoom >= 0.5 && self.options.grid {
            let grid = values(&self.project.json["defs"]["layers"])
                .iter()
                .filter(|d| d["type"] != "Entities")
                .map(|d| number(d, "gridSize", 16.))
                .reduce(f32::min)
                .unwrap_or(number(&self.project.json, "defaultGridSize", 16.));
            ([grid, grid], 0.5 * ((zoom - 0.5) / 0.5).min(1.))
        } else {
            ([0., 0.], 0.)
        };
        let base = theme::color(text(&self.project.json, "bgColor", "#292c39"));
        let target = Color32::from_rgb(129, 135, 189);
        let color = Color32::from_rgb(
            (base.r() as f32 * 0.15 + target.r() as f32 * 0.85) as u8,
            (base.g() as f32 * 0.15 + target.g() as f32 * 0.85) as u8,
            (base.b() as f32 * 0.15 + target.b() as f32 * 0.85) as u8,
        )
        .gamma_multiply(alpha);
        let origin = self.point([0., 0.]);
        for (axis, spacing) in spacing.iter().enumerate() {
            let step = spacing * zoom;
            if step < 2. {
                continue;
            }
            let start = if axis == 0 {
                self.canvas.left()
            } else {
                self.canvas.top()
            };
            let end = if axis == 0 {
                self.canvas.right()
            } else {
                self.canvas.bottom()
            };
            let off = if axis == 0 { origin.x } else { origin.y };
            let mut p = start + (off - start).rem_euclid(step);
            while p <= end {
                let line = if axis == 0 {
                    [
                        egui::pos2(p, self.canvas.top()),
                        egui::pos2(p, self.canvas.bottom()),
                    ]
                } else {
                    [
                        egui::pos2(self.canvas.left(), p),
                        egui::pos2(self.canvas.right(), p),
                    ]
                };
                painter.line_segment(line, Stroke::new(1.0_f32, color));
                p += step;
            }
        }
        if self.options.details && matches!(layout, "Free" | "GridVania") {
            painter.line_segment(
                [
                    egui::pos2(self.canvas.left(), origin.y),
                    egui::pos2(self.canvas.right(), origin.y),
                ],
                Stroke::new(3.0_f32, Color32::WHITE.gamma_multiply(0.15)),
            );
            painter.line_segment(
                [
                    egui::pos2(origin.x, self.canvas.top()),
                    egui::pos2(origin.x, self.canvas.bottom()),
                ],
                Stroke::new(3.0_f32, Color32::WHITE.gamma_multiply(0.15)),
            );
        }
    }
    fn level_annotation_point(&self, world: [f32; 2]) -> Pos2 {
        // The identifier and field flow are children of WorldRender, whose
        // backing-pixel origin is rounded independently from LevelRender.
        if self.options.world {
            return self.point(world);
        }
        let mut view = self.view.clone();
        if let Some(level) = self.project.level(self.options.selected) {
            view.center[0] += number(level, "worldX", 0.);
            view.center[1] += number(level, "worldY", 0.);
        }
        let origin = view.level_render_canvas_origin(self.canvas, self.ctx.pixels_per_point());
        view.to_screen(world, origin, self.canvas.size().into())
            .into()
    }

    fn level_identifier(
        &mut self,
        painter: &Painter,
        key: LevelKey,
        level: &Value,
        rect: Rect,
        origin: [f32; 2],
    ) -> f32 {
        let backing_zoom = self.view.zoom * self.ctx.pixels_per_point();
        if self.options.world && backing_zoom < 0.03 {
            return 0.;
        }
        let name = text(level, "identifier", "Level");
        let modified = level["iid"].as_str().is_some_and(|iid| {
            self.options
                .modified_levels
                .is_some_and(|levels| levels.contains(iid))
        });
        let error =
            crate::app::bleep::level_entity_error(self.ctx, self.project, key).or_else(|| {
                let background = level["bgRelPath"].as_str()?;
                self.assets
                    .errors
                    .contains_key(&asset_path(self.project, background))
                    .then(|| "Bg image".to_owned())
            });
        let marker = if modified { "*" } else { "" };
        let label = match &error {
            Some(error) => std::borrow::Cow::Owned(format!("{name}{marker} <ERR: {error}>")),
            None if modified => std::borrow::Cow::Owned(format!("{name}*")),
            None => std::borrow::Cow::Borrowed(name),
        };
        let color = self.level_color(level);
        let world = self
            .options
            .selected
            .world
            .and_then(|i| self.project.json["worlds"].get(i))
            .unwrap_or(&self.project.json);
        let layout = text(world, "worldLayout", "Free");
        let text_size = bitmap_font::measure(self.ctx, &label, 1., f32::INFINITY);
        let scale = if matches!(layout, "Free" | "GridVania") {
            (rect.width() / (text_size.x + 12.)).min(1.)
        } else {
            1.
        };
        let size = (text_size + egui::vec2(12., 0.)) * scale;
        let min = if self.options.world {
            let [x, y] = origin;
            let width = number(level, "pxWid", 256.);
            let height = number(level, "pxHei", 256.);
            let size = size / self.view.zoom;
            let position = match layout {
                "LinearHorizontal" => [x + width * 0.3, y - size.y],
                "LinearVertical" => [x - size.x - 30., y + (height - size.y) * 0.5],
                _ => [x + 2., y + 2.],
            };
            self.level_annotation_point(position.map(f32::trunc))
        } else {
            let x = number(level, "worldX", 0.) + number(level, "pxWid", 256.) * 0.5;
            let y = number(level, "worldY", 0.);
            self.level_annotation_point([
                (x - size.x * 0.5 / self.view.zoom).trunc(),
                (y - size.y / self.view.zoom - 12.).trunc(),
            ])
        };
        let mut painter = painter.clone();
        if self.options.world {
            let fade_start = 0.03 * 0.8;
            painter.multiply_opacity(((backing_zoom - fade_start) / fade_start).clamp(0., 1.));
        }
        let angle: f32 = if self.options.world && layout == "LinearHorizontal" {
            -0.4
        } else {
            0.
        };
        let rotate = |v: egui::Vec2| {
            let (sin, cos) = angle.sin_cos();
            egui::vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
        };
        painter.add(egui::Shape::convex_polygon(
            vec![
                min,
                min + rotate(egui::vec2(size.x, 0.)),
                min + rotate(size),
                min + rotate(egui::vec2(0., size.y)),
            ],
            darken(color, 0.3).gamma_multiply(if level["useAutoIdentifier"] == true {
                0.4
            } else {
                1.
            }),
            Stroke::NONE,
        ));
        bitmap_font::paint_rotated(
            self.ctx,
            &painter,
            self.assets,
            min + rotate(egui::vec2(6., -2.) * scale),
            &label,
            if error.is_some() {
                Color32::RED
            } else {
                lighten(color, 0.5)
            },
            scale,
            f32::INFINITY,
            false,
            angle,
        );
        size.y
    }
    pub fn screen(&self, r: [f32; 4]) -> Rect {
        Rect::from_min_max(self.point([r[0], r[1]]), self.point([r[2], r[3]]))
    }
    pub fn point(&self, p: [f32; 2]) -> Pos2 {
        self.view
            .to_screen(p, self.projection_origin, self.canvas.size().into())
            .into()
    }
    pub fn texture(&mut self, tileset: &Value) -> Option<egui::TextureHandle> {
        match tileset["relPath"].as_str() {
            Some(path) => self
                .assets
                .texture(self.ctx, &asset_path(self.project, path)),
            None if !tileset["embedAtlas"].is_null() => self.assets.embedded(self.ctx),
            None => None,
        }
    }
    fn layer_transform(
        &self,
        level: &Value,
        layer: &Value,
        def: &Value,
        origin: [f32; 2],
    ) -> Transform {
        layer_transform_for(level, layer, def, origin, self.view, self.options.parallax)
    }
    fn layer_alpha(&self, layer: &Value, def: &Value, active: bool) -> f32 {
        let base = number(def, "displayOpacity", number(layer, "__opacity", 1.));
        base * if active {
            1.
        } else {
            number(def, "inactiveOpacity", 1.)
                * if self.options.single_layer {
                    0.8 - 0.75 * self.options.single_layer_intensity.clamp(0., 1.)
                } else {
                    1.
                }
        }
    }
    fn tile_base_tint(&self, inactive: bool) -> Color32 {
        let mut color = if self.options.tile_tags {
            Color32::from_rgb(204, 204, 204)
        } else {
            Color32::WHITE
        };
        if inactive && self.options.single_layer && !blur::available() {
            let k = self.options.single_layer_intensity.clamp(0., 1.);
            color = Color32::from_rgb(
                (color.r() as f32 * (1. - k) + 140. * k) as u8,
                (color.g() as f32 * (1. - k) + 153. * k) as u8,
                (color.b() as f32 * (1. - k) + 193. * k) as u8,
            );
        }
        color
    }
    fn background(
        &mut self,
        painter: &Painter,
        level: &Value,
        origin: [f32; 2],
    ) -> Option<blur::LayerRaster> {
        let path = level["bgRelPath"].as_str()?;
        let texture = self
            .assets
            .texture(self.ctx, &asset_path(self.project, path))?;
        let size = texture.size_vec2();
        let bounds = Rect::from_min_size(
            origin.into(),
            egui::vec2(number(level, "pxWid", 256.), number(level, "pxHei", 256.)),
        );
        let src = Rect::from_min_size(Pos2::ZERO, size);
        let pivot = [
            number(level, "bgPivotX", 0.5),
            number(level, "bgPivotY", 0.5),
        ];
        let mode = match text(level, "bgPos", "Unscaled") {
            "Contain" => "FitInside",
            "Cover" => "Cover",
            "CoverDirty" => "Stretch",
            "Repeat" => "Repeat",
            _ => "FullSizeCropped",
        };
        let alpha = if self.options.single_layer && !self.options.world {
            0.8 - 0.75 * self.options.single_layer_intensity.clamp(0., 1.)
        } else {
            1.
        };
        let filtered = alpha < 1. && blur::available();
        let mut repeat_anchor = None;
        let quads = if mode == "Repeat" {
            let cropped = size.min(bounds.size());
            let source = Rect::from_min_size(
                ((size - cropped) * egui::vec2(pivot[0], pivot[1])).to_pos2(),
                cropped,
            );
            let world = |point: Pos2| {
                self.view
                    .to_world(
                        point.into(),
                        self.projection_origin,
                        self.canvas.size().into(),
                    )
                    .into()
            };
            let clip = painter.clip_rect();
            // Keep an extra period at the edges for inverse projection rounding.
            let visible = Rect::from_min_max(world(clip.min), world(clip.max)).expand2(cropped);
            let quads = geometry::aligned_repeat(bounds, source, pivot, visible);
            if filtered && !quads.is_empty() {
                // Keep the blur raster aligned to the first tile in the full background.
                let first_region = Rect::from_min_size(bounds.min, cropped);
                repeat_anchor = geometry::aligned_repeat(bounds, source, pivot, first_region)
                    .into_iter()
                    .next();
            }
            quads
        } else {
            let mut quads = geometry::entity_tiles(bounds, src, pivot, mode, [0.; 4]);
            for q in &mut quads {
                let off = q.dst.min - bounds.min;
                let min = bounds.min + egui::vec2(off.x.trunc(), off.y.trunc());
                q.dst = Rect::from_min_size(min, q.dst.size());
            }
            quads
        };
        let raster = repeat_anchor
            .as_ref()
            .or_else(|| quads.first())
            .filter(|quad| filtered && quad.src.is_positive() && quad.dst.is_positive())
            .map(|quad| blur::LayerRaster {
                origin: self.point(quad.dst.min.into()),
                points_per_pixel: (quad.dst.size() / quad.src.size()) * self.view.zoom,
                opacity: alpha,
            });
        let draw_alpha = if raster.is_some() { 1. } else { alpha };
        let tint = Color32::WHITE.gamma_multiply(draw_alpha);
        let mut mesh = Mesh::with_texture(texture.id());
        for q in quads {
            mesh.add_rect_with_uv(
                self.screen([q.dst.left(), q.dst.top(), q.dst.right(), q.dst.bottom()]),
                Rect::from_min_max(
                    (q.src.min.to_vec2() / size).to_pos2(),
                    (q.src.max.to_vec2() / size).to_pos2(),
                ),
                tint,
            );
        }
        painter.add(egui::Shape::mesh(mesh));
        raster
    }
    fn intgrid(
        &self,
        painter: &Painter,
        layer: &Value,
        def: &Value,
        transform: Transform,
        alpha: f32,
    ) -> usize {
        let grid = number(layer, "__gridSize", 16.).max(1.);
        let width = number(layer, "__cWid", 0.).max(0.) as usize;
        if width == 0 {
            return 0;
        }
        let cells = values(&layer["intGridCsv"]);
        let height = cells.len().div_ceil(width);
        let min = self.view.to_world(
            painter.clip_rect().min.into(),
            self.projection_origin,
            self.canvas.size().into(),
        );
        let max = self.view.to_world(
            painter.clip_rect().max.into(),
            self.projection_origin,
            self.canvas.size().into(),
        );
        let x0 = (((min[0] - transform.origin[0]) / transform.scale / grid)
            .floor()
            .max(0.) as usize)
            .min(width);
        let x1 = (((max[0] - transform.origin[0]) / transform.scale / grid)
            .ceil()
            .max(0.) as usize)
            .min(width);
        let y0 = (((min[1] - transform.origin[1]) / transform.scale / grid)
            .floor()
            .max(0.) as usize)
            .min(height);
        let y1 = (((max[1] - transform.origin[1]) / transform.scale / grid)
            .ceil()
            .max(0.) as usize)
            .min(height);
        let pending = self
            .options
            .async_paint
            .and_then(|preview| preview.layers.get(&layer["layerDefUid"].as_i64()?));
        let cells = pending
            .and_then(|p| p.old_intgrid_cells.as_deref())
            .unwrap_or(cells);
        let mut overrides: Vec<_> = pending
            .into_iter()
            .flat_map(|preview| &preview.old_intgrid)
            .filter_map(|(&[x, y], &value)| {
                let (x, y) = (x as usize, y as usize);
                ((x0..x1).contains(&x) && (y0..y1).contains(&y)).then(|| (y * width + x, value))
            })
            .collect();
        // Brush updates are keyed by column and row. The painter walks rows first.
        overrides.sort_unstable_by_key(|&(index, _)| index);
        let mut overrides = overrides.into_iter().peekable();
        let screen_size = self.canvas.size();
        let screen_center = [
            self.projection_origin[0] + screen_size.x * 0.5,
            self.projection_origin[1] + screen_size.y * 0.5,
        ];
        let screen_edges = |axis: usize, index: usize| {
            let pixel = index as f32 * grid;
            [pixel, pixel + grid].map(|edge| {
                let world = transform.origin[axis] + edge * transform.scale;
                screen_center[axis] + (world - self.view.center[axis]) * self.view.zoom
            })
        };
        let columns: Vec<_> = (x0..x1).map(|x| screen_edges(0, x)).collect();
        let mut cell_color = intgrid_cell_colors(def, alpha);
        let mut count = 0;
        let shapes = (y0..y1)
            .flat_map(|y| {
                let [top, bottom] = screen_edges(1, y);
                let row = y * width;
                (x0..x1)
                    .zip(columns.iter().copied())
                    .map(move |(x, [left, right])| (row + x, [left, top, right, bottom]))
            })
            .filter_map(|(index, [left, top, right, bottom])| {
                let value = if overrides.peek().is_some_and(|&(cell, _)| cell == index) {
                    overrides.next().unwrap().1
                } else {
                    cells.get(index).and_then(Value::as_i64).unwrap_or(0)
                };
                if value == 0 {
                    return None;
                }
                count += 1;
                let rect = Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
                Some(egui::Shape::rect_filled(rect, 0., cell_color(value)))
            });
        if painter.opacity() == 0. || !painter.is_visible() {
            // The batched API skips hidden painters without consuming the iterator.
            for shape in shapes {
                painter.add(shape);
            }
        } else {
            painter.extend(shapes);
        }
        count
    }
    #[allow(clippy::too_many_arguments)]
    fn missing_atlas_tiles(
        &mut self,
        painter: &Painter,
        level: &Value,
        layer: &Value,
        def: &Value,
        transform: Transform,
        alpha: f32,
        tiles: &[Value],
    ) -> usize {
        let grid = def["gridSize"]
            .as_i64()
            .or_else(|| layer["__gridSize"].as_i64())
            .unwrap_or(16)
            .max(1);
        let [width, height] = layer_pixel_size(level, layer);
        if width <= 0 || height <= 0 || alpha <= 0. || tiles.is_empty() {
            return 0;
        }
        let Some(texture) = self.assets.unavailable_classic_atlas(self.ctx, grid) else {
            return 0;
        };
        // The level renderer places its error markers in the layer mask using only the X pivot.
        let base = transform.offset([
            -number(layer, "__pxTotalOffsetX", 0.),
            -number(layer, "__pxTotalOffsetY", 0.),
        ]);
        let columns = 1 + (width - 1) / grid;
        let rows = 1 + (height - 1) / grid;
        let side = grid as f32;
        let shift = egui::Vec2::splat(number(def, "tilePivotX", 0.) * side);
        let mut occupied = HashSet::new();
        let mut mesh = Mesh::with_texture(texture.id());
        let uv = Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.));
        let tint = Color32::WHITE.gamma_multiply(alpha);
        for tile in tiles {
            let [x, y] = ldtk_core::exports::classic_tile_cell(tile, columns, grid);
            if x < 0 || x >= columns || y < 0 || y >= rows || !occupied.insert([x, y]) {
                continue;
            }
            let position = egui::pos2(x as f32 * side, y as f32 * side) + shift;
            let destination =
                self.screen(base.rect(Rect::from_min_size(position, egui::Vec2::splat(side))));
            if destination.intersects(painter.clip_rect()) {
                mesh.add_rect_with_uv(destination, uv, tint);
            }
        }
        let count = mesh.vertices.len() / 4;
        painter.add(egui::Shape::mesh(mesh));
        count
    }
    #[allow(clippy::too_many_arguments)]
    fn tiles_in_region(
        &mut self,
        painter: &Painter,
        level: &Value,
        layer: &Value,
        def: &Value,
        transform: Transform,
        alpha: f32,
        active: bool,
        region: Option<Rect>,
    ) -> usize {
        let grid = number(layer, "__gridSize", 16.).max(1.);
        let uid = layer["overrideTilesetUid"]
            .as_i64()
            .or_else(|| layer["__tilesetDefUid"].as_i64())
            .or_else(|| def["tilesetDefUid"].as_i64());
        let grid_tiles = self
            .options
            .async_paint
            .and_then(|preview| preview.layers.get(&layer["layerDefUid"].as_i64()?))
            .and_then(|preview| preview.old_tiles.as_deref())
            .unwrap_or_else(|| values(&layer["gridTiles"]));
        let tileset = uid.and_then(|uid| self.project.tileset(uid));
        let Some((tileset, texture)) =
            tileset.and_then(|tileset| self.texture(tileset).map(|texture| (tileset, texture)))
        else {
            if region.is_none() && layer["__type"] == "IntGrid" {
                return self.intgrid(painter, layer, def, transform, alpha);
            }
            if region.is_some()
                || layer["__type"] != "Tiles"
                || tileset
                    .and_then(|td| td["relPath"].as_str())
                    .is_some_and(|path| {
                        !self
                            .assets
                            .errors
                            .contains_key(&asset_path(self.project, path))
                    })
            {
                return 0;
            }
            return self
                .missing_atlas_tiles(painter, level, layer, def, transform, alpha, grid_tiles);
        };
        let tile_size = number(tileset, "tileGridSize", grid);
        let atlas = texture.size_vec2().into();
        let pivot = [number(def, "tilePivotX", 0.), number(def, "tilePivotY", 0.)];
        let clip = painter.clip_rect();
        let mut mesh = Mesh::with_texture(texture.id());
        let mut count = 0;
        let tile_groups = [values(&layer["autoLayerTiles"]), grid_tiles];
        let base_tint = self.tile_base_tint(!active);
        let tag_opacity = if self.options.tile_tags { 0.5 } else { 1. };
        let mut opaque_tint = None;
        let unshifted =
            region.is_none() && tile_size.is_finite() && tile_size == grid && pivot == [0.; 2];
        let screen_center = self.projection_origin;
        let screen_size = self.canvas.size();
        let screen_center = [
            screen_center[0] + screen_size.x * 0.5,
            screen_center[1] + screen_size.y * 0.5,
        ];
        let screen_axis = |axis: usize, px: f32| {
            let world = transform.origin[axis] + px * transform.scale;
            screen_center[axis] + (world - self.view.center[axis]) * self.view.zoom
        };
        let shifted_destination =
            |tile: &Value, anchor: &Value, offsets: &[geometry::LayerTileOffset; 4]| {
                let anchor = values(anchor);
                let x = anchor.first().and_then(Value::as_f64).unwrap_or(0.) as f32;
                if region.is_some_and(|r| x < r.left() || x >= r.right()) {
                    return None;
                }
                let flips = tile["f"].as_i64().unwrap_or(0);
                let offset = &offsets[(flips & 3) as usize];
                let x = offset.apply(0, x);
                let left = screen_axis(0, x);
                if left > clip.right() {
                    return None;
                }
                let right = screen_axis(0, x + tile_size);
                if right < clip.left() {
                    return None;
                }
                let y = anchor.get(1).and_then(Value::as_f64).unwrap_or(0.) as f32;
                if region.is_some_and(|r| y < r.top() || y >= r.bottom()) {
                    return None;
                }
                let y = offset.apply(1, y);
                Some(Rect::from_min_max(
                    egui::pos2(left, screen_axis(1, y)),
                    egui::pos2(right, screen_axis(1, y + tile_size)),
                ))
            };
        for (index, group) in tile_groups.into_iter().enumerate() {
            let auto = index == 0;
            if auto && !self.options.render_auto {
                continue;
            }
            let offsets = (!unshifted).then(|| {
                std::array::from_fn::<_, 4, _>(|flips| {
                    geometry::LayerTileOffset::new(grid, tile_size, pivot, flips as i64, auto)
                })
            });
            for tile in group {
                let anchor = tile_anchor(tile);
                let dst = if let Some(offsets) = &offsets {
                    let Some(dst) = shifted_destination(tile, anchor, offsets) else {
                        continue;
                    };
                    dst
                } else {
                    // A zero pivot and matching grids make the destination independent of flips.
                    let anchor = values(anchor);
                    let x = anchor.first().and_then(Value::as_f64).unwrap_or(0.) as f32;
                    let left = screen_axis(0, x);
                    if left > clip.right() {
                        continue;
                    }
                    let right = screen_axis(0, x + tile_size);
                    if right < clip.left() {
                        continue;
                    }
                    let y = anchor.get(1).and_then(Value::as_f64).unwrap_or(0.) as f32;
                    Rect::from_min_max(
                        egui::pos2(left, screen_axis(1, y)),
                        egui::pos2(right, screen_axis(1, y + tile_size)),
                    )
                };
                if !dst.intersects(clip) {
                    continue;
                }
                let mut source = [0.; 2];
                let mut flips = 0;
                let mut opacity = 1.;
                // Read the small tile record once after culling.
                for (key, value) in tile.as_object().into_iter().flatten() {
                    match key.as_str() {
                        "src" => source = pair(value, [0.; 2]),
                        "f" => flips = value.as_i64().unwrap_or(0),
                        "a" => opacity = value.as_f64().unwrap_or(1.) as f32,
                        _ => (),
                    }
                }
                let [min, max] = tile_uv(source, [tile_size; 2], atlas, flips);
                let tint = if opacity == 1. {
                    *opaque_tint
                        .get_or_insert_with(|| base_tint.gamma_multiply(alpha * tag_opacity))
                } else {
                    base_tint.gamma_multiply(alpha * opacity * tag_opacity)
                };
                mesh.add_rect_with_uv(dst, Rect::from_min_max(min.into(), max.into()), tint);
                count += 1;
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        if self.options.tile_tags {
            self.tile_tags(painter, tile_groups, def, tileset, transform, grid);
        }
        count
    }
    fn tile_tags(
        &self,
        painter: &Painter,
        tile_groups: [&[Value]; 2],
        def: &Value,
        tileset: &Value,
        transform: Transform,
        grid: f32,
    ) {
        let Some(enum_uid) = tileset["tagsSourceEnumUid"].as_i64() else {
            return;
        };
        let Some(en) = values(&self.project.json["defs"]["enums"])
            .iter()
            .chain(values(&self.project.json["defs"]["externalEnums"]))
            .find(|e| e["uid"] == enum_uid)
        else {
            return;
        };
        let tags = values(&tileset["enumTags"]);
        let enum_values = values(&en["values"]);
        if tags.is_empty() || enum_values.is_empty() {
            return;
        }
        let mut colors_by_tile = std::collections::HashMap::new();
        let offset = [
            number(def, "tilePivotX", 0.) * grid,
            number(def, "tilePivotY", 0.) * grid,
        ];
        let clip = painter.clip_rect();
        let cull = self
            .ctx
            .tessellation_options(|options| options.coarse_tessellation_culling);
        let screen_center: [f32; 2] =
            (Pos2::from(self.projection_origin) + self.canvas.size() * 0.5).into();
        let project = |axis: usize, point: f32| {
            let world = transform.origin[axis] + point * transform.scale;
            screen_center[axis] + (world - self.view.center[axis]) * self.view.zoom
        };
        let rect = |min: [f32; 2], max: [f32; 2]| {
            Rect::from_min_max(
                egui::pos2(project(0, min[0]), project(1, min[1])),
                egui::pos2(project(0, max[0]), project(1, max[1])),
            )
        };
        let box_size = grid - 1.;
        let stroke_width = transform.scale * self.view.zoom;
        let position = |tile| {
            let [x, y] = pair(tile_anchor(tile), [0.; 2]);
            [x + offset[0], y + offset[1]]
        };
        let intersects = |[x, y]: [f32; 2], inset: usize| {
            let inset = inset as f32;
            rect(
                [x + 0.5, y + 0.5],
                [x + inset + 0.5 + box_size, y + inset + 0.5 + box_size],
            )
            .intersects(clip)
        };
        for tile in tile_groups.into_iter().flatten() {
            let clipped_position = cull.then(|| position(tile));
            // Every enum value can contribute at most one border.
            if clipped_position.is_some_and(|pos| !intersects(pos, enum_values.len() - 1)) {
                continue;
            }
            let id = &tile["t"];
            let colors = colors_by_tile.entry(id).or_insert_with(|| {
                enum_values
                    .iter()
                    .filter(|value| {
                        tags.iter().any(|tag| {
                            tag["enumValueId"] == value["id"]
                                && values(&tag["tileIds"]).contains(id)
                        })
                    })
                    .map(|value| theme::color(text(value, "color", "#ffcc00")))
                    .collect::<Vec<_>>()
            });
            if colors.is_empty() {
                continue;
            }
            let at = clipped_position.unwrap_or_else(|| position(tile));
            if cull && !intersects(at, colors.len() - 1) {
                continue;
            }
            let [x, y] = at;
            for (n, &color) in colors.iter().enumerate() {
                let n = n as f32;
                let min = [x + n + 0.5, y + n + 0.5];
                let side = box_size - n * 2.;
                let r = rect(min, [min[0] + side, min[1] + side]);
                if cull && !r.intersects(clip) {
                    continue;
                }
                painter.rect_stroke(r, 0., Stroke::new(stroke_width, color), StrokeKind::Inside);
            }
        }
    }
    fn grid(
        &self,
        painter: &Painter,
        level: &Value,
        layer: &Value,
        def: &Value,
        transform: Transform,
    ) {
        let w = number(level, "pxWid", 256.);
        let h = number(level, "pxHei", 256.);
        let grid = number(layer, "__gridSize", 16.).max(1.);
        let viewport_min = self.view.to_world(
            painter.clip_rect().min.into(),
            self.projection_origin,
            self.canvas.size().into(),
        );
        let viewport_max = self.view.to_world(
            painter.clip_rect().max.into(),
            self.projection_origin,
            self.canvas.size().into(),
        );
        let bg = theme::color(text(&self.project.json, "bgColor", "#40465B"));
        let luminance = (0.299 * (bg.r() as f32 / 255.).powi(2)
            + 0.587 * (bg.g() as f32 / 255.).powi(2)
            + 0.114 * (bg.b() as f32 / 255.).powi(2))
        .sqrt();
        let base = if luminance >= 0.8 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        for (axis, limit) in [(0, w), (1, h)] {
            for (step, color) in [
                (grid, base.gamma_multiply(0.07)),
                (
                    if self.options.guides {
                        number(
                            def,
                            if axis == 0 {
                                "guideGridWid"
                            } else {
                                "guideGridHei"
                            },
                            0.,
                        )
                    } else {
                        0.
                    },
                    guide_grid_color(def, base),
                ),
            ] {
                if step <= 1. || step * self.view.zoom * transform.scale < 4. {
                    continue;
                }
                let start =
                    (((viewport_min[axis] - transform.origin[axis]) / transform.scale / step)
                        .floor()
                        .max(0.)) as i32;
                let end = (((viewport_max[axis] - transform.origin[axis]) / transform.scale / step)
                    .ceil()
                    .min(limit / step)) as i32;
                for i in start..=end {
                    let p = i as f32 * step;
                    let (a, b) = if axis == 0 {
                        ([p, 0.], [p, h])
                    } else {
                        ([0., p], [w, p])
                    };
                    painter.line_segment(
                        [
                            self.point(transform.point(a)),
                            self.point(transform.point(b)),
                        ],
                        Stroke::new(1.0_f32, color),
                    );
                }
            }
        }
    }
    fn rulers(
        &mut self,
        painter: &Painter,
        level: &Value,
        layer: Option<&Value>,
        origin: [f32; 2],
    ) {
        let w = number(level, "pxWid", 256.);
        let h = number(level, "pxHei", 256.);
        let bg = theme::color(text(&self.project.json, "bgColor", "#40465B"));
        let [r, g, b] = [bg.r(), bg.g(), bg.b()].map(|v| (u16::from(v) + 255).div_ceil(2) as u8);
        let label_color = Color32::from_rgb(r, g, b);
        let horizontal = layer.map_or_else(
            || format!("{w}px"),
            |l| format!("{} cells / {w}px", number(l, "__cWid", 0.)),
        );
        let vertical = layer.map_or_else(
            || format!("{h}px"),
            |l| format!("{} cells / {h}px", number(l, "__cHei", 0.)),
        );
        for (label, anchor, extent, vertical) in [
            (&horizontal, [(w * 0.5).trunc(), h + 20.], w, false),
            (&vertical, [-20., (h * 0.5).trunc()], h, true),
            (&vertical, [w + 20., (h * 0.5).trunc()], h, true),
        ] {
            if !vertical && fields::level_has_beneath_label(self.project, level) {
                continue;
            }
            let local_scale = ruler_scale(self.ctx, 1., extent);
            let size = bitmap_font::measure(self.ctx, label, local_scale, f32::INFINITY);
            // Heaps truncates the text's offset before applying the level zoom.
            let offset = egui::vec2((-size.x * 0.5).trunc(), (-size.y * 0.5).trunc());
            let (offset, angle) = if vertical {
                (
                    egui::vec2(offset.y, -offset.x),
                    -std::f32::consts::FRAC_PI_2,
                )
            } else {
                (offset, 0.)
            };
            bitmap_font::paint_rotated(
                self.ctx,
                painter,
                self.assets,
                self.point([
                    origin[0] + anchor[0] + offset.x,
                    origin[1] + anchor[1] + offset.y,
                ]),
                label,
                label_color,
                local_scale * self.view.zoom,
                f32::INFINITY,
                false,
                angle,
            );
        }
    }
}

fn ruler_scale(ctx: &egui::Context, zoom: f32, extent: f32) -> f32 {
    zoom * ctx.pixels_per_point() * if extent < 200. { 0.5 } else { 1. }
}

fn level_bounds_distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    ((a[0] - b[2]).max(0.) + (b[0] - a[2]).max(0.))
        .max((a[1] - b[3]).max(0.) + (b[1] - a[3]).max(0.))
}

fn paint_level_hover(ctx: &egui::Context, painter: &Painter, rect: Rect) {
    painter.rect_filled(rect, 0., Color32::from_rgba_premultiplied(38, 31, 0, 38));
    painter.rect_stroke(
        rect,
        0.,
        Stroke::new(2. / ctx.pixels_per_point(), Color32::WHITE),
        StrokeKind::Middle,
    );
}

pub fn draw(
    painter: &Painter,
    canvas: Rect,
    project: &Project,
    view: &Viewport,
    options: &RenderOptions<'_>,
    linear_drag: Option<(LevelKey, [i32; 2])>,
    assets: &mut Assets,
) -> usize {
    let ctx = painter.ctx();
    painter.rect_filled(
        canvas,
        0.,
        theme::color(text(&project.json, "bgColor", "#292c39")),
    );
    let canvas_painter = painter.with_clip_rect(canvas);
    blur::next_frame(options.single_layer);
    let projection_origin = view.level_render_canvas_origin(canvas, ctx.pixels_per_point());
    let mut scene = Scene {
        ctx,
        project,
        view,
        canvas,
        projection_origin,
        options,
        assets,
        entity_index: fields::EntityIndex::new(project),
        file_states: Default::default(),
    };
    if options.world {
        scene.world_background(painter);
        scene.world_grid(painter);
    }
    let selected_level = project.level(options.selected);
    let selected_origin = selected_level
        .map(|l| [number(l, "worldX", 0.), number(l, "worldY", 0.)])
        .unwrap_or([0.; 2]);
    let selected_bounds = bounds(project, options.selected, false);
    let active_depth = options
        .world_depth
        .or_else(|| selected_level.and_then(|l| l["worldDepth"].as_i64()))
        .unwrap_or(0);
    let mut world_bounds = EMPTY_BOUNDS;
    let mut levels: Vec<_> = iter_placements(project, options.selected)
        .filter_map(|(key, level, mut bounds)| {
            if options.world {
                if options.details {
                    world_bounds = union_bounds(world_bounds, bounds);
                }
                if let Some((dragged, position)) = linear_drag
                    && dragged == key
                {
                    let [x, y] = position.map(|value| value as f32);
                    bounds = [x, y, x + bounds[2] - bounds[0], y + bounds[3] - bounds[1]];
                }
            } else {
                if key == options.selected {
                    return None;
                }
                for axis in 0..2 {
                    bounds[axis] -= selected_origin[axis];
                    bounds[axis + 2] -= selected_origin[axis];
                }
            }
            let rect = scene.screen(bounds);
            if !rect.expand(64.).intersects(canvas) {
                return None;
            }
            let depth = level["worldDepth"].as_i64().unwrap_or(0);
            Some((key, level, bounds, rect, depth))
        })
        .collect();
    levels.sort_by_key(|(_, _, _, _, depth)| std::cmp::Reverse(*depth));
    let selected_rect = scene.screen(selected_bounds);
    if !options.world
        && let Some(level) = selected_level
        && selected_rect.expand(64.).intersects(canvas)
    {
        levels.push((
            options.selected,
            level,
            selected_bounds,
            selected_rect,
            level["worldDepth"].as_i64().unwrap_or(0),
        ));
    }
    let mut count = 0;
    let mut current_highlight = None;
    let mut hover_rect = None;
    let mut world_level_fields = Vec::new();
    for (key, level, bounds, rect, depth) in levels {
        if !options.world
            && key == options.selected
            && let Some(rect) = hover_rect.take()
        {
            paint_level_hover(ctx, painter, rect);
        }
        let depth_delta = depth - active_depth;
        let bg_alpha = if options.world && depth_delta < 0 {
            0.
        } else if options.world && depth_delta > 0 {
            0.6 * if depth_delta >= 2 { 0.3 } else { 1. }
        } else {
            1.
        };
        let render_alpha = if options.world && depth_delta < 0 {
            0.
        } else if options.world && depth_delta > 0 {
            0.15
        } else {
            1.
        };
        let outline_alpha = if options.world && depth_delta < 0 {
            0.45 * if depth_delta <= -2 { 0.3 } else { 1. }
        } else if options.world && depth_delta > 0 {
            0.2
        } else {
            1.
        };
        let nearby = !options.world && key != options.selected;
        let origin = [bounds[0], bounds[1]];
        if nearby {
            let distance = level_bounds_distance(bounds, selected_bounds);
            let mut neighbor_options = options.clone();
            neighbor_options.single_layer = false;
            neighbor_options.tile_tags = false;
            neighbor_options.render_auto = true;
            neighbor_options.parallax = false;
            neighbor_options.async_paint = None;
            let mut neighbor = Scene {
                ctx,
                project,
                view,
                canvas,
                projection_origin,
                options: &neighbor_options,
                assets: scene.assets,
                entity_index: fields::EntityIndex::new(project),
                file_states: Default::default(),
            };
            let mut content = painter.with_clip_rect(canvas.intersect(rect));
            let bg = theme::color(text(
                level,
                "__bgColor",
                text(&project.json, "defaultLevelBgColor", "#696A79"),
            ));
            if depth_delta >= 0 {
                let opacity = 0.2
                    * if depth_delta == 0 {
                        1.
                    } else {
                        0.6 * if depth_delta >= 2 { 0.3 } else { 1. }
                    };
                content.multiply_opacity(opacity);
                content.rect_filled(rect, 0., bg);
                neighbor.background(&content, level, origin);
                let edge_region = if depth_delta == 0 && distance == 0. && options.nearby_dist > 0.
                {
                    Some(Rect::from_min_max(
                        egui::pos2(
                            selected_bounds[0] - origin[0] - options.nearby_dist,
                            selected_bounds[1] - origin[1] - options.nearby_dist,
                        ),
                        egui::pos2(
                            selected_bounds[2] - origin[0] + options.nearby_dist,
                            selected_bounds[3] - origin[1] + options.nearby_dist,
                        ),
                    ))
                } else {
                    None
                };
                if distance <= 300. {
                    let mut layers = canvas_painter.clone();
                    layers.multiply_opacity(if depth_delta > 0 { 0.15 } else { 1. });
                    count += neighbor.world_layers(&layers, level, origin, edge_region);
                    if let Some(region) = edge_region {
                        for (index, layer) in
                            values(&level["layerInstances"]).iter().enumerate().rev()
                        {
                            let kind = text(layer, "__type", "");
                            if kind == "Entities"
                                || (kind == "IntGrid" && layer["__tilesetDefUid"].is_null())
                            {
                                continue;
                            }
                            let def = project
                                .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
                                .unwrap_or(&Value::Null);
                            let alpha = number(def, "displayOpacity", 1.)
                                * if index == options.active_layer {
                                    1.
                                } else {
                                    number(def, "inactiveOpacity", 1.)
                                };
                            let transform = Transform { origin, scale: 1. }.offset([
                                number(layer, "__pxTotalOffsetX", 0.),
                                number(layer, "__pxTotalOffsetY", 0.),
                            ]);
                            count += neighbor.tiles_in_region(
                                &layers,
                                level,
                                layer,
                                def,
                                transform,
                                alpha,
                                true,
                                Some(region),
                            );
                        }
                    }
                }
                painter.rect_filled(rect, 0., bg.gamma_multiply(0.3));
            }
            if options.details {
                let opacity = 0.3
                    * if depth_delta < 0 {
                        0.45 * if depth_delta <= -2 { 0.3 } else { 1. }
                    } else if depth_delta > 0 {
                        0.2
                    } else {
                        1.
                    };
                painter.rect_stroke(
                    rect.shrink(view.zoom),
                    0.,
                    Stroke::new(2.0_f32, neighbor.level_color(level).gamma_multiply(opacity)),
                    StrokeKind::Inside,
                );
                if depth_delta == 0 {
                    let min_zoom = (canvas.width() / selected_bounds[2] / 2.3)
                        .min(canvas.height() / selected_bounds[3] / 2.3)
                        * 0.8;
                    let mut label_painter = painter.clone();
                    label_painter
                        .multiply_opacity(((view.zoom - min_zoom) / min_zoom).clamp(0., 1.));
                    neighbor.level_identifier(&label_painter, key, level, rect, origin);
                }
            }
            if options.world_hover == Some(key) && depth_delta == 0 {
                hover_rect = Some(rect);
            }
            continue;
        }
        {
            painter.rect_filled(
                rect,
                0.,
                theme::color(text(
                    level,
                    "__bgColor",
                    text(&project.json, "defaultLevelBgColor", "#696A79"),
                ))
                .gamma_multiply(bg_alpha),
            );
        }
        let clip = canvas.intersect(rect);
        let clipped = painter.with_clip_rect(clip);
        {
            let first = blur::begin(painter);
            let mut bg_painter = clipped.clone();
            bg_painter.multiply_opacity(bg_alpha);
            if let Some(raster) = scene.background(&bg_painter, level, origin) {
                blur::finish(
                    painter,
                    first,
                    canvas,
                    options.single_layer_intensity,
                    (number(level, "uid", 0.) as u64) << 32,
                    raster,
                );
            }
        }
        let layers = values(&level["layerInstances"]);
        if options.world {
            if render_alpha > 0. {
                let mut layer_painter = canvas_painter.clone();
                layer_painter.multiply_opacity(render_alpha);
                count += scene.world_layers(&layer_painter, level, origin, None);
            }
        } else {
            for (index, layer) in layers.iter().enumerate().rev() {
                let uid = layer["layerDefUid"].as_i64().unwrap_or(-1);
                let def = project.layer_def(uid).unwrap_or(&Value::Null);
                if options.hidden.contains(&uid) || layer["visible"].as_bool() == Some(false) {
                    continue;
                }
                let kind = text(layer, "__type", "");
                let visible_in_tiles_only = layer_visible_in_tiles_only(kind, def);
                if !options.details && !visible_in_tiles_only {
                    continue;
                }
                let active = key == options.selected && index == options.active_layer;
                let alpha = scene.layer_alpha(layer, def, active);
                let filtered = options.single_layer && !active && blur::available();
                let draw_alpha = if filtered { 1. } else { alpha };
                let base = scene.layer_transform(level, layer, def, origin);
                let transform = base.offset([
                    number(layer, "__pxTotalOffsetX", 0.),
                    number(layer, "__pxTotalOffsetY", 0.),
                ]);
                let first = blur::begin(painter);
                if kind == "Entities" {
                    let entities = options
                        .async_paint
                        .filter(|preview| preview.level == key)
                        .and_then(|preview| preview.layers.get(&uid))
                        .and_then(|preview| preview.old_entities.as_deref())
                        .unwrap_or_else(|| values(&layer["entityInstances"]));
                    for index in entity_order(entities, def) {
                        let entity = &entities[index];
                        scene.entity(painter, entity, def, transform, draw_alpha, active, key);
                    }
                } else {
                    // LayerRender anchors the mask at the layer root before applying tile offsets.
                    let [width, height] = layer_pixel_size(level, layer);
                    if width <= 0 || height <= 0 {
                        continue;
                    }
                    let mask =
                        Rect::from_min_size(Pos2::ZERO, egui::vec2(width as f32, height as f32));
                    let layer_painter =
                        canvas_painter.with_clip_rect(scene.screen(base.rect(mask)));
                    let autotile = kind != "Tiles" && visible_in_tiles_only;
                    if kind == "IntGrid" && (!autotile || !options.render_auto) {
                        count += scene.intgrid(&layer_painter, layer, def, transform, draw_alpha);
                    }
                    if kind == "Tiles" || (autotile && options.render_auto) {
                        count += scene.tiles_in_region(
                            &layer_painter,
                            level,
                            layer,
                            def,
                            transform,
                            draw_alpha,
                            active,
                            None,
                        );
                    }
                }
                if filtered {
                    blur::finish(
                        painter,
                        first,
                        canvas,
                        options.single_layer_intensity,
                        ((number(level, "uid", 0.) as u64) << 32) ^ (uid as u64 + 1),
                        blur::LayerRaster {
                            origin: scene.point(transform.origin),
                            points_per_pixel: egui::Vec2::splat(view.zoom * transform.scale),
                            opacity: alpha,
                        },
                    );
                }
            }
        }
        if key == options.selected && !options.world {
            if let Some(preview) = options.async_paint
                && let Some(grid) = preview.hint_grid
            {
                for (&[x, y], &color) in &preview.hints {
                    let cell = Rect::from_min_size(
                        [x as f32 * grid, y as f32 * grid].into(),
                        egui::Vec2::splat(grid),
                    );
                    let dst = scene.screen(Transform { origin, scale: 1. }.rect(cell));
                    if dst.intersects(clipped.clip_rect()) {
                        let color = color.gamma_multiply(0.7);
                        let additive = Color32::from_rgb_additive(color.r(), color.g(), color.b());
                        clipped.rect_filled(dst, 0., additive);
                    }
                }
            }
            if options.grid
                && options.details
                && let Some(layer) = layers.get(options.active_layer)
            {
                let def = project
                    .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
                    .unwrap_or(&Value::Null);
                let transform = scene.layer_transform(level, layer, def, origin).offset([
                    number(layer, "__pxTotalOffsetX", 0.),
                    number(layer, "__pxTotalOffsetY", 0.),
                ]);
                scene.grid(&clipped, level, layer, def, transform);
            }
            // Heaps draws a one-level-pixel rectangle without a screen-space fringe.
            let outer = rect.expand(view.zoom * 0.5);
            let inner = rect.shrink(view.zoom * 0.5);
            let color = Color32::WHITE.gamma_multiply(0.7);
            let mut edges = egui::Mesh::default();
            for edge in [
                Rect::from_min_max(outer.min, egui::pos2(outer.right(), inner.top())),
                Rect::from_min_max(egui::pos2(outer.left(), inner.bottom()), outer.max),
                Rect::from_min_max(egui::pos2(outer.left(), inner.top()), inner.left_bottom()),
                Rect::from_min_max(inner.right_top(), egui::pos2(outer.right(), inner.bottom())),
            ] {
                edges.add_colored_rect(edge, color);
            }
            painter.add(egui::Shape::mesh(edges));
            // Original boundsGlow is added after bounds in DP_UI: its
            // knockout shadow also darkens the outer half of the white edge.
            shadow::paint(painter, rect, view.zoom);
            if options.rulers && options.details {
                scene.rulers(painter, level, layers.get(options.active_layer), origin);
            }
        }
        {
            if options.world {
                if options.details {
                    painter.rect_stroke(
                        rect.shrink(view.zoom),
                        0.,
                        Stroke::new(
                            if key == options.selected {
                                3.0_f32
                            } else {
                                2.0_f32
                            },
                            scene.level_color(level).gamma_multiply(outline_alpha),
                        ),
                        StrokeKind::Inside,
                    );
                }
                if key == options.selected && depth_delta == 0 && !options.world_multi_selection {
                    current_highlight = Some(rect);
                }
                if options.world_hover == Some(key) && depth_delta == 0 {
                    hover_rect = Some(rect);
                }
            }
            if options.details && (!options.world || depth_delta == 0) {
                let title_height = scene.level_identifier(painter, key, level, rect, origin);
                if options.world {
                    world_level_fields.push((level, origin));
                } else {
                    scene.level_fields(painter, level, origin, title_height);
                }
            }
        }
    }
    if let Some(rect) = hover_rect {
        paint_level_hover(ctx, painter, rect);
    }
    if options.world {
        for (level, origin) in world_level_fields {
            scene.level_fields(painter, level, origin, 0.);
        }
        if let Some(rect) = current_highlight {
            // WorldRender puts the current-level outline above the field flows.
            let width = if options.details { 4. } else { 1. };
            painter.rect_stroke(
                rect.expand(width * 0.5),
                0.,
                Stroke::new(width, theme::ACCENT),
                StrokeKind::Middle,
            );
        }
        scene.world_title(painter, world_bounds);
    }
    count
}

pub fn entity_preview(
    ctx: &egui::Context,
    painter: &Painter,
    rect: Rect,
    project: &Project,
    definition: &Value,
    assets: &mut Assets,
) {
    entities::preview(ctx, painter, rect, project, definition, assets);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhostMode {
    Move,
    EntityDrag,
    Copy,
    Brush,
    EntityHover { ui_scale: f32 },
}

/// Render only the clipboard payload; no project or source-layer clone is required.
#[allow(clippy::too_many_arguments)]
pub fn draw_group_ghost(
    ctx: &egui::Context,
    painter: &Painter,
    canvas: Rect,
    project: &Project,
    view: &Viewport,
    key: LevelKey,
    group: &Value,
    delta_px: [f32; 2],
    flips: [bool; 2],
    mode: GhostMode,
    assets: &mut Assets,
) {
    let Some(level) = project.level(key) else {
        return;
    };
    let hidden = HashSet::new();
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
        single_layer_intensity: 0.,
        fields_table: false,
        world_depth: None,
        nearby_dist: 0.,
        parallax: true,
        rulers: false,
        guides: false,
        async_paint: None,
    };
    let mut scene = Scene {
        ctx,
        project,
        view,
        canvas,
        projection_origin: view.level_render_canvas_origin(canvas, ctx.pixels_per_point()),
        options: &options,
        assets,
        entity_index: fields::EntityIndex::new(project),
        file_states: Default::default(),
    };
    let painter = painter.with_clip_rect(canvas);
    let origin = pair(&group["origin"], [0.; 2]);
    let size = pair(&group["size"], [0.; 2]);
    for entry in values(&group["layers"]).iter().rev() {
        let Some((index, layer)) = values(&level["layerInstances"])
            .iter()
            .enumerate()
            .find(|(_, l)| l["layerDefUid"] == entry["layerDefUid"])
        else {
            continue;
        };
        let def = project
            .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
            .unwrap_or(&Value::Null);
        let transform = if matches!(
            mode,
            GhostMode::EntityDrag | GhostMode::Brush | GhostMode::EntityHover { .. }
        ) {
            pointer_transform(project, key, index, view)
        } else {
            editing_transform(project, key, index, view)
        };
        let transform = transform.unwrap_or(Transform {
            origin: [0.; 2],
            scale: 1.,
        });
        let entity_transform = if mode == GhostMode::EntityDrag {
            Transform {
                scale: 1.,
                ..transform
            }
        } else {
            transform
        };
        let offset = [
            number(layer, "__pxTotalOffsetX", 0.),
            number(layer, "__pxTotalOffsetY", 0.),
        ];
        let source_origin = pair(&entry["origin"], [0.; 2]);
        let clipboard = &entry["clipboard"];
        let grid = number(clipboard, "gridSize", number(layer, "__gridSize", 16.)).max(1.);
        let width = number(clipboard, "width", 0.) as usize;
        let geometry_aware = group["geometryAware"] == true;
        let scaled = grid * number(&entry["geometry"], "scale", 1.0);
        let smart = number(group, "scaledGridSize", scaled).max(f32::EPSILON);
        let raw_delta = if text(layer, "__type", "") == "Entities" {
            delta_px.map(f32::trunc)
        } else {
            delta_px.map(|d| (d / smart).trunc() * (smart / scaled).trunc() * grid)
        };
        let point_delta = delta_px.map(|d| (d / scaled).trunc() * grid);
        let (move_origin, move_size, move_delta) = if geometry_aware {
            (
                source_origin,
                [width as f32 * grid, number(clipboard, "height", 0.) * grid],
                raw_delta,
            )
        } else {
            (origin, size, delta_px)
        };
        let moved_axis = |axis: usize, point: f32, extent: f32| {
            let relative = point - move_origin[axis];
            let relative = if flips[axis] {
                move_size[axis] - relative - extent
            } else {
                relative
            };
            move_origin[axis] + move_delta[axis] + relative
        };
        let moved = |point: [f32; 2], extent: [f32; 2]| -> [f32; 2] {
            std::array::from_fn(|axis| moved_axis(axis, point[axis], extent[axis]))
        };

        let cells = values(&clipboard["intGridCsv"]);
        if width > 0 && !cells.is_empty() {
            let brush = mode == GhostMode::Brush;
            let radius = if brush {
                view.zoom * transform.scale
            } else {
                0.
            };
            let clip = painter.clip_rect();
            let min: [f32; 2] = clip.min.into();
            let max: [f32; 2] = clip.max.into();
            let screen_size: [f32; 2] = scene.canvas.size().into();
            let edges = |axis: usize, cell: usize| {
                let point = source_origin[axis] + cell as f32 * grid;
                let local = moved_axis(axis, point, grid) - offset[axis];
                [local, local + grid].map(|edge| {
                    let world = transform.origin[axis] + edge * transform.scale;
                    scene.projection_origin[axis]
                        + screen_size[axis] * 0.5
                        + (world - view.center[axis]) * view.zoom
                })
            };
            // Match the rectangle tessellator's bounds for a centered brush stroke.
            let visible = |axis: usize, [start, end]: [f32; 2]| {
                start - radius <= max[axis] && end + radius >= min[axis]
            };
            // Visible columns are contiguous even when the selection is mirrored.
            let mut columns = (0..width.min(cells.len())).filter(|&x| visible(0, edges(0, x)));
            if let Some(first) = columns.next() {
                let end = columns.next_back().unwrap_or(first) + 1;
                let mut cell_color = intgrid_cell_colors(def, 1.);
                for (y, row) in cells.chunks(width).enumerate() {
                    let vertical @ [top, bottom] = edges(1, y);
                    if !visible(1, vertical) {
                        continue;
                    }
                    for (x, value) in row.iter().enumerate().take(end).skip(first) {
                        let value = value.as_i64().unwrap_or(0);
                        if value == 0 {
                            continue;
                        }
                        let [left, right] = edges(0, x);
                        let rect =
                            Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom));
                        let color = cell_color(value);
                        if brush {
                            painter.rect_stroke(
                                rect.shrink(2. * radius),
                                0.,
                                Stroke::new(2. * radius, Color32::BLACK),
                                StrokeKind::Middle,
                            );
                            painter.rect_stroke(
                                rect,
                                0.,
                                Stroke::new(2. * radius, color),
                                StrokeKind::Middle,
                            );
                        } else {
                            painter.rect_filled(rect, 0., color);
                        }
                    }
                }
            }
        }
        let tiles = values(&clipboard["gridTiles"]);
        if !tiles.is_empty() {
            let flip_mask = i64::from(flips[0]) | (i64::from(flips[1]) << 1);
            scene.ghost_tiles(
                &painter,
                layer,
                def,
                tiles,
                transform,
                grid,
                mode,
                flip_mask,
                |axis, pixel| moved_axis(axis, source_origin[axis] + pixel, grid) - offset[axis],
            );
        }
        let source_offset = [
            source_origin[0] - entry["rect"][0].as_f64().unwrap_or(0.) as f32 * grid,
            source_origin[1] - entry["rect"][1].as_f64().unwrap_or(0.) as f32 * grid,
        ];
        let remap_point = |value: &Value| {
            let point = [
                source_offset[0] + number(value, "cx", 0.) * grid,
                source_offset[1] + number(value, "cy", 0.) * grid,
            ];
            let dst = if geometry_aware {
                std::array::from_fn(|axis| point[axis] + point_delta[axis])
            } else {
                moved(point, [grid; 2])
            };
            serde_json::json!({"cx":((dst[0]-offset[0])/grid).floor(),"cy":((dst[1]-offset[1])/grid).floor()})
        };
        let mut moved_entities = std::collections::HashMap::new();
        for entity in values(&clipboard["entityInstances"]) {
            let mut e = entity.clone();
            let px = pair(&entity["px"], [0.; 2]);
            let mut dst = moved(
                [source_origin[0] + px[0], source_origin[1] + px[1]],
                [0.; 2],
            );
            let pivot = pair(&entity["__pivot"], [0.; 2]);
            for axis in 0..2 {
                if flips[axis] {
                    dst[axis] += ((2. * pivot[axis] - 1.)
                        * number(entity, if axis == 0 { "width" } else { "height" }, 0.))
                    .trunc();
                }
            }
            e["px"] = serde_json::json!([dst[0] - offset[0], dst[1] - offset[1]]);
            for point in values(&entry["points"])
                .iter()
                .filter(|p| p["entityIid"] == entity["iid"])
            {
                if let Some(field) = e["fieldInstances"]
                    .as_array_mut()
                    .and_then(|fs| fs.iter_mut().find(|f| f["defUid"] == point["fieldDefUid"]))
                {
                    let mapped = remap_point(&point["value"]);
                    if field["__type"] == "Array<Point>" {
                        let i = point["arrayIndex"].as_u64().unwrap_or(0) as usize;
                        if let Some(value) = field["__value"].get_mut(i) {
                            *value = mapped;
                        }
                    } else {
                        field["__value"] = mapped;
                    }
                }
            }
            scene.entity(
                &painter,
                &e,
                def,
                entity_transform,
                match mode {
                    GhostMode::Brush | GhostMode::EntityHover { .. } => 0.33,
                    GhostMode::Move | GhostMode::EntityDrag | GhostMode::Copy => 0.5,
                },
                true,
                key,
            );
            if matches!(mode, GhostMode::Move | GhostMode::EntityDrag) {
                let entity_def = values(&project.json["defs"]["entities"])
                    .iter()
                    .find(|d| d["uid"] == e["defUid"])
                    .unwrap_or(&Value::Null);
                let (owner, link_origin) = if mode == GhostMode::EntityDrag {
                    (
                        entity,
                        Transform {
                            origin: delta_px,
                            scale: 1.,
                        },
                    )
                } else {
                    (&e, transform)
                };
                scene.ghost_entity_links(&painter, owner, entity_def, link_origin, transform, grid);
            } else if let GhostMode::EntityHover { ui_scale } = mode {
                scene.hovered_entity_cursor(&painter, &e, transform, ui_scale);
            }
            if let Some(iid) = e["iid"].as_str() {
                moved_entities.insert(iid.to_owned(), e);
            }
        }
        for point in values(&entry["points"]) {
            let Some(entity) = values(&layer["entityInstances"])
                .iter()
                .find(|e| e["iid"] == point["entityIid"])
            else {
                continue;
            };
            let entity_def = values(&project.json["defs"]["entities"])
                .iter()
                .find(|d| d["uid"] == entity["defUid"])
                .unwrap_or(&Value::Null);
            let Some(fd) = values(&entity_def["fieldDefs"])
                .iter()
                .find(|f| f["uid"] == point["fieldDefUid"])
            else {
                continue;
            };
            scene.ghost_point(
                &painter,
                entity,
                entity_def,
                fd,
                point,
                entry,
                entity["iid"]
                    .as_str()
                    .and_then(|iid| moved_entities.get(iid)),
                transform,
                grid,
                mode,
                remap_point,
            );
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;
