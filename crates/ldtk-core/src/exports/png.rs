use super::{
    ExportBatch, arr, color, embedded_atlas, encode_png, load_image_rgba, n, num, s, safe_name,
};
use crate::{Error, LevelKey, Project, Result};
use image::{Rgba, RgbaImage};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngExportMode {
    OneImagePerLayer,
    OneImagePerLevel,
    LayersAndLevels,
}
#[derive(Clone, Debug)]
pub struct PngExportOptions {
    pub mode: PngExportMode,
    pub include_background: bool,
    pub file_pattern: Option<String>,
}
impl Default for PngExportOptions {
    fn default() -> Self {
        Self {
            mode: PngExportMode::OneImagePerLayer,
            include_background: true,
            file_pattern: None,
        }
    }
}
/// Scale and cropped size in source pixels for one level background tile.
pub fn background_image_layout(
    level: [f64; 2],
    image: [f64; 2],
    mode: &str,
) -> ([f64; 2], [f64; 2]) {
    let x = level[0] / image[0];
    let y = level[1] / image[1];
    let scale = match mode {
        "Contain" => [x.min(y); 2],
        "Cover" => [x.max(y); 2],
        "CoverDirty" => [x, y],
        _ => [1.; 2],
    };
    let crop = std::array::from_fn(|axis| image[axis].min(level[axis] / scale[axis]));
    (scale, crop)
}

fn blend_rgba(d: &mut Rgba<u8>, c: Rgba<u8>, alpha: f64) {
    let a = c[3] as f64 / 255.0 * alpha.clamp(0., 1.);
    if a == 0. {
        return;
    }
    if a == 1. {
        *d = c;
        return;
    }
    let da = d[3] as f64 / 255.;
    let out = a + da * (1. - a);
    if out > 0. {
        for i in 0..3 {
            d[i] = ((c[i] as f64 * a + d[i] as f64 * da * (1. - a)) / out).round() as u8;
        }
    }
    d[3] = (out * 255.).round() as u8;
}
fn premultiply(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = f64::from(pixel[3]) / 255.;
        for channel in &mut pixel.0[..3] {
            *channel = (f64::from(*channel) * alpha) as u8;
        }
    }
}

fn composite(target: &mut RgbaImage, source: &RgbaImage, alpha: f64, premultiplied: bool) {
    if premultiplied {
        for (dst, src) in target.pixels_mut().zip(source.pixels()) {
            // The preview quantizes opacity before blending the 8-bit color channels.
            let source_alpha = (f64::from(src[3]) * alpha) as u16;
            let dest_weight = 255 - source_alpha;
            for channel in 0..3 {
                let color =
                    u16::from(src[channel]) * source_alpha + u16::from(dst[channel]) * dest_weight;
                dst[channel] = ((color + 127) / 255) as u8;
            }
            dst[3] = (source_alpha + u16::from(dst[3]) * dest_weight / 255) as u8;
        }
    } else {
        debug_assert_eq!(target.dimensions(), source.dimensions());
        for (dst, src) in target.pixels_mut().zip(source.pixels()) {
            blend_rgba(dst, *src, alpha);
        }
    }
}

fn tile_fits([width, height]: [i64; 2], [x, y]: [i64; 2], size: i64) -> bool {
    x >= 0 && y >= 0 && x <= width - size && y <= height - size
}

fn visible_tile_pixels(
    origin: i64,
    clip: i64,
    source: i64,
    atlas: i64,
    size: i64,
    flipped: bool,
) -> Range<i64> {
    // Match the source and destination bounds in tile-local coordinates.
    let source_range = if flipped {
        source.saturating_add(size - atlas)..source.saturating_add(size)
    } else {
        source.saturating_neg()..atlas.saturating_sub(source)
    };
    origin.saturating_neg().max(source_range.start).max(0)
        ..clip.saturating_sub(origin).min(source_range.end).min(size)
}

fn opaque_atlas_tile(atlas: &RgbaImage, [x, y]: [i64; 2], size: i64) -> bool {
    let width = atlas.width() as usize * 4;
    let offset = x as usize * 4;
    let bytes = size as usize * 4;
    (y..y + size).all(|row| {
        let start = row as usize * width + offset;
        atlas.as_raw()[start..start + bytes]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
    })
}

fn copy_opaque_tile(
    target: &mut RgbaImage,
    atlas: &RgbaImage,
    [tx, ty]: [i64; 2],
    [sx, sy]: [i64; 2],
    size: i64,
    flips: i64,
) {
    let width = target.width() as usize * 4;
    let atlas_width = atlas.width() as usize * 4;
    let pixels = target.as_flat_samples_mut().samples;
    let source = atlas.as_raw();
    let bytes = size as usize * 4;
    for row in 0..size {
        let source_y = sy + if flips & 2 == 0 { row } else { size - 1 - row };
        let src = source_y as usize * atlas_width + sx as usize * 4;
        let dst = (ty + row) as usize * width + tx as usize * 4;
        let src = &source[src..src + bytes];
        let dst = &mut pixels[dst..dst + bytes];
        if flips & 1 == 0 {
            dst.copy_from_slice(src);
        } else {
            let (dst_pixels, _) = dst.as_chunks_mut::<4>();
            let (src_pixels, _) = src.as_chunks::<4>();
            for (to, from) in dst_pixels.iter_mut().zip(src_pixels.iter().rev()) {
                *to = *from;
            }
        }
    }
}

/// Find the occupied cell in a classic Tiles layer with a positive column count and grid size.
pub fn classic_tile_cell(tile: &Value, columns: i64, grid: i64) -> [i64; 2] {
    if let Some(coord) = tile["d"][0].as_i64() {
        [coord % columns, coord / columns]
    } else {
        [0, 1].map(|axis| tile["px"][axis].as_i64().unwrap_or(-1).div_euclid(grid))
    }
}

/// Red channel of the source editor's opaque missing-atlas marker.
pub fn unavailable_classic_tile_red(grid: i64, x: i64, y: i64) -> u8 {
    let size = grid as f64;
    let x = x as f64 + 0.5;
    let y = y as f64 + 0.5;
    let on_stroke = |y: f64| {
        (x - y).abs() < std::f64::consts::SQRT_2 && (0.4 * size..=1.6 * size).contains(&(x + y))
    };
    if on_stroke(y) || on_stroke(size - y) {
        255
    } else {
        136
    }
}

fn draw_unavailable_classic_atlas(
    image: &mut RgbaImage,
    layer: &Value,
    def: &Value,
    opacity: f64,
) -> bool {
    let grid = def["gridSize"]
        .as_i64()
        .unwrap_or_else(|| n(layer, "__gridSize"))
        .max(1);
    let width = i64::from(image.width()).saturating_sub(n(layer, "pxOffsetX"));
    let height = i64::from(image.height()).saturating_sub(n(layer, "pxOffsetY"));
    if width <= 0 || height <= 0 || opacity == 0. {
        return false;
    }
    let columns = 1 + (width - 1) / grid;
    let rows = 1 + (height - 1) / grid;
    let clip_x = width.min(i64::from(image.width()));
    let clip_y = height.min(i64::from(image.height()));
    // The original uses the X pivot on both axes and draws inside the layer mask.
    let shift = (num(def, "tilePivotX", 0.) * grid as f64 - 0.5).ceil() as i64;
    let alpha = (255. * opacity) as u8;
    let mut visited = HashSet::new();
    let mut painted = false;
    for tile in arr(&layer["gridTiles"]) {
        let [cx, cy] = classic_tile_cell(tile, columns, grid);
        if cx < 0 || cx >= columns || cy < 0 || cy >= rows || !visited.insert((cx, cy)) {
            continue;
        }
        let left = (cx * grid).saturating_add(shift);
        let top = (cy * grid).saturating_add(shift);
        let x_range = left.max(0)..left.saturating_add(grid).min(clip_x);
        let y_range = top.max(0)..top.saturating_add(grid).min(clip_y);
        if x_range.is_empty() || y_range.is_empty() {
            continue;
        }
        painted = true;
        for y in y_range {
            for x in x_range.clone() {
                let red = unavailable_classic_tile_red(grid, x - left, y - top);
                image.put_pixel(
                    x as u32,
                    y as u32,
                    Rgba([(f64::from(red) * opacity) as u8, 0, 0, alpha]),
                );
            }
        }
    }
    painted
}

impl Project {
    pub fn export_png(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_png_with_progress(dir, |_, _| {})
    }
    /// Reports completed level renders and sends (0, 0) when the stage ends.
    pub fn export_png_with_progress(
        &self,
        dir: impl AsRef<Path>,
        progress: impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        self.export_png_with_diagnostics(dir, progress, &mut Vec::new())
    }
    pub(crate) fn export_png_with_diagnostics(
        &self,
        dir: impl AsRef<Path>,
        mut progress: impl FnMut(usize, usize),
        diagnostics: &mut Vec<String>,
    ) -> Result<Vec<PathBuf>> {
        let mode = match s(&self.json, "imageExportMode") {
            "OneImagePerLevel" => PngExportMode::OneImagePerLevel,
            "LayersAndLevels" => PngExportMode::LayersAndLevels,
            _ => PngExportMode::OneImagePerLayer,
        };
        self.export_png_options_with_progress(
            dir,
            &PngExportOptions {
                mode,
                include_background: self.json["exportLevelBg"].as_bool().unwrap_or(true),
                file_pattern: self.json["pngFilePattern"].as_str().map(str::to_owned),
            },
            &mut progress,
            diagnostics,
        )
    }
    pub fn export_png_with_options(
        &self,
        dir: impl AsRef<Path>,
        options: &PngExportOptions,
    ) -> Result<Vec<PathBuf>> {
        self.export_png_options_with_progress(dir, options, &mut |_, _| {}, &mut Vec::new())
    }
    fn export_png_options_with_progress(
        &self,
        dir: impl AsRef<Path>,
        options: &PngExportOptions,
        progress: &mut impl FnMut(usize, usize),
        diagnostics: &mut Vec<String>,
    ) -> Result<Vec<PathBuf>> {
        let mut batch = ExportBatch::new(self);
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        let mut renderer = PngRenderer::new(self);
        for (index, key) in keys.into_iter().enumerate() {
            renderer.stage_level_png(key, dir.as_ref(), options, false, diagnostics, &mut batch)?;
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        batch.publish()
    }
    fn png_name(&self, key: LevelKey, li: &Value, pattern: &str) -> Result<String> {
        let l = self.level(key).unwrap();
        let world = key
            .world
            .and_then(|w| self.json["worlds"].get(w))
            .map(|w| s(w, "identifier"))
            .unwrap_or("World");
        let idx = arr(&self.json["defs"]["layers"])
            .iter()
            .take_while(|d| n(d, "uid") != n(li, "layerDefUid"))
            .filter(|d| s(d, "type") != "Entities")
            .count();
        let result = pattern
            .replace("%level_name", s(l, "identifier"))
            .replace("%layer_name", s(li, "__identifier"))
            .replace("%world", world)
            .replace("%level_idx", &format!("{:04}", key.level))
            .replace("%layer_idx", &format!("{idx:02}"));
        safe_name(&result)?;
        Ok(result)
    }
    fn resource_image(&self, rel: &str) -> Result<RgbaImage> {
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));
        let path = base.join(rel);
        load_image_rgba(&path)
    }
    fn level_background_color(&self, l: &Value) -> Rgba<u8> {
        color(
            l["bgColor"]
                .as_str()
                .or(self.json["defaultLevelBgColor"].as_str())
                .or(l["__bgColor"].as_str())
                .unwrap_or("#000000"),
        )
    }
    fn render_background(&self, l: &Value, diagnostics: &mut Vec<String>) -> Result<RgbaImage> {
        let (w, h) = dimensions(l)?;
        let background_color = self.level_background_color(l);
        if let Some(rel) = l["bgRelPath"].as_str() {
            let mut bg = match self.resource_image(rel) {
                Ok(bg) => bg,
                Err(error) => {
                    diagnostics.push(format!(
                        "Failed to load background {rel} for level {}: {error}",
                        s(l, "identifier")
                    ));
                    return Ok(RgbaImage::from_pixel(w, h, background_color));
                }
            };
            if bg.dimensions() == (w, h) {
                // All positioning modes sample the same pixel when the dimensions match.
                for pixel in bg.pixels_mut() {
                    if pixel[3] != 255 {
                        let mut blended = background_color;
                        blend_rgba(&mut blended, *pixel, 1.);
                        *pixel = blended;
                    }
                }
                return Ok(bg);
            }
            let mut image = RgbaImage::from_pixel(w, h, background_color);
            let bw = bg.width() as f64;
            let bh = bg.height() as f64;
            let mode = s(l, "bgPos");
            let ([sx, sy], [cw, ch]) =
                background_image_layout([f64::from(w), f64::from(h)], [bw, bh], mode);
            let pivot_x = num(l, "bgPivotX", 0.5);
            let pivot_y = num(l, "bgPivotY", 0.5);
            let cx = pivot_x * (bw - cw);
            let cy = pivot_y * (bh - ch);
            let repeat = mode == "Repeat";
            let (x, y) = if repeat {
                ((w as f64 - bw) * pivot_x, (h as f64 - bh) * pivot_y)
            } else {
                (
                    (pivot_x * (w as f64 - cw * sx)).trunc(),
                    (pivot_y * (h as f64 - ch * sy)).trunc(),
                )
            };
            let sample =
                |pixel: u32, offset: f64, scale: f64, crop: f64, span: f64, extent: f64| {
                    let relative = f64::from(pixel) - offset;
                    let coordinate = if repeat {
                        relative.rem_euclid(extent)
                    } else {
                        relative / scale + crop
                    };
                    (coordinate >= crop
                        && coordinate < crop + span
                        && coordinate >= 0.
                        && coordinate < extent)
                        .then_some(coordinate as u32)
                };
            // Reuse each horizontal mapping across rows without allocating for the full width.
            const SAMPLE_COLUMNS: u32 = 4096;
            let mut columns = Vec::with_capacity(w.min(SAMPLE_COLUMNS) as usize);
            for start in (0..w).step_by(SAMPLE_COLUMNS as usize) {
                columns.clear();
                columns.extend(
                    (start..w.min(start + SAMPLE_COLUMNS))
                        .filter_map(|dx| sample(dx, x, sx, cx, cw, bw).map(|bx| (dx, bx))),
                );
                if columns.is_empty() {
                    continue;
                }
                for dy in 0..h {
                    let Some(by) = sample(dy, y, sy, cy, ch, bh) else {
                        continue;
                    };
                    for &(dx, bx) in &columns {
                        blend_rgba(image.get_pixel_mut(dx, dy), *bg.get_pixel(bx, by), 1.);
                    }
                }
            }
            return Ok(image);
        }
        Ok(RgbaImage::from_pixel(w, h, background_color))
    }
}
/// Keep the two most recent atlases when both fit in the decoded pixel budget.
pub(super) struct PngRenderer<'a> {
    project: &'a Project,
    last_atlas: Option<(i64, RgbaImage)>,
    previous_atlas: Option<(i64, RgbaImage)>,
}

impl<'a> PngRenderer<'a> {
    pub(super) fn new(project: &'a Project) -> Self {
        Self {
            project,
            last_atlas: None,
            previous_atlas: None,
        }
    }

    fn tile_atlas(&mut self, uid: Option<i64>) -> Result<Option<(&'a Value, &RgbaImage)>> {
        let Some(uid) = uid else {
            return Ok(None);
        };
        let Some(def) = self.project.tileset(uid) else {
            return Ok(None);
        };
        if self.last_atlas.as_ref().map(|(id, _)| *id) != Some(uid) {
            if self
                .previous_atlas
                .as_ref()
                .is_some_and(|(id, _)| *id == uid)
            {
                std::mem::swap(&mut self.last_atlas, &mut self.previous_atlas);
            } else {
                // Keep peak residency at most the last atlas and the newly decoded image.
                self.previous_atlas = None;
                let image = if let Some(rel) = def["relPath"].as_str() {
                    self.project.resource_image(rel)?
                } else if !def["embedAtlas"].is_null() {
                    embedded_atlas()?
                } else {
                    return Err(Error::Invalid("missing atlas path".into()));
                };
                const DECODED_ATLAS_BUDGET: usize = 16 * 1024 * 1024;
                self.previous_atlas = self.last_atlas.take().filter(|(_, previous)| {
                    previous.as_raw().len().saturating_add(image.as_raw().len())
                        <= DECODED_ATLAS_BUDGET
                });
                self.last_atlas = Some((uid, image));
            }
        }
        let (_, image) = self.last_atlas.as_ref().expect("loaded tileset");
        Ok(Some((def, image)))
    }

    pub(super) fn stage_level_png(
        &mut self,
        key: LevelKey,
        dir: &Path,
        options: &PngExportOptions,
        simple: bool,
        diagnostics: &mut Vec<String>,
        batch: &mut ExportBatch<'_>,
    ) -> Result<()> {
        let project = self.project;
        let l = project
            .level(key)
            .ok_or_else(|| Error::Invalid("missing external level".into()))?;
        let (w, h) = dimensions(l)?;
        let name = safe_name(s(l, "identifier"))?;
        let mut combined = if options.include_background {
            let background = project.render_background(l, diagnostics)?;
            if options.mode != PngExportMode::OneImagePerLevel {
                let filename = if simple {
                    "_bg.png".into()
                } else {
                    format!("{name}_bg.png")
                };
                batch.add(dir.join(filename), &encode_png(&background)?)?;
            }
            match options.mode {
                PngExportMode::OneImagePerLevel => Some(background),
                PngExportMode::LayersAndLevels => Some(RgbaImage::from_pixel(
                    w,
                    h,
                    project.level_background_color(l),
                )),
                PngExportMode::OneImagePerLayer => None,
            }
        } else {
            (options.mode != PngExportMode::OneImagePerLayer).then(|| RgbaImage::new(w, h))
        };
        let mut premultiplied = false;
        for li in arr(&l["layerInstances"]).iter().rev() {
            let def = project.layer_def(n(li, "layerDefUid")).unwrap_or(li);
            let typ = s(li, "__type");
            let tiles = typ == "Tiles"
                || typ == "AutoLayer"
                || (typ == "IntGrid" && li["__tilesetDefUid"].as_i64().is_some());
            let pattern = options.file_pattern.as_deref().unwrap_or(
                if options.mode == PngExportMode::OneImagePerLevel {
                    "%level_name"
                } else {
                    "%level_name__%layer_name"
                },
            );
            let file = project.png_name(key, li, pattern)?;
            if tiles {
                let opacity = num(def, "displayOpacity", 1.).clamp(0., 1.);
                let layer_file = options.mode != PngExportMode::OneImagePerLevel;
                let (image, placeholder) =
                    self.render_tile_layer(l, li, if layer_file { opacity } else { 1. })?;
                if let Some(combined) = &mut combined {
                    if placeholder && opacity > 0. && !premultiplied {
                        premultiply(combined);
                        premultiplied = true;
                    }
                    let alpha = if options.mode == PngExportMode::LayersAndLevels || placeholder {
                        opacity
                    } else {
                        1.
                    };
                    composite(combined, &image, alpha, premultiplied);
                }
                if layer_file {
                    batch.add(dir.join(format!("{file}.png")), &encode_png(&image)?)?;
                }
            }
            if typ == "IntGrid" && options.mode != PngExportMode::OneImagePerLevel {
                let img = intgrid_image(li, def)?;
                batch.add(dir.join(format!("{file}-int.png")), &encode_png(&img)?)?;
            }
        }
        if let Some(combined) = combined {
            let filename = if simple {
                "_composite".into()
            } else if options.mode == PngExportMode::OneImagePerLevel {
                let li = arr(&l["layerInstances"]).first().unwrap_or(l);
                project.png_name(
                    key,
                    li,
                    options.file_pattern.as_deref().unwrap_or("%level_name"),
                )?
            } else {
                name.to_owned()
            };
            batch.add(dir.join(format!("{filename}.png")), &encode_png(&combined)?)?;
        }
        Ok(())
    }

    fn render_tile_layer(
        &mut self,
        l: &Value,
        li: &Value,
        placeholder_opacity: f64,
    ) -> Result<(RgbaImage, bool)> {
        let (w, h) = dimensions(l)?;
        let mut img = RgbaImage::new(w, h);
        let def = self.project.layer_def(n(li, "layerDefUid")).unwrap_or(li);
        let classic = s(li, "__type") == "Tiles";
        let uid = li["__tilesetDefUid"]
            .as_i64()
            .or_else(|| def["tilesetDefUid"].as_i64());
        let (td, atlas) = match self.tile_atlas(uid) {
            Ok(Some(atlas)) => atlas,
            _ if classic => {
                let painted =
                    draw_unavailable_classic_atlas(&mut img, li, def, placeholder_opacity);
                return Ok((img, painted));
            }
            Ok(None) => return Ok((img, false)),
            Err(error) => return Err(error),
        };
        let size = n(td, "tileGridSize").max(1);
        // The layer mask uses the instance offset. Tile positions use the total offset.
        let clip = [(w, "pxOffsetX"), (h, "pxOffsetY")].map(|(extent, field)| {
            let extent = i64::from(extent);
            extent.saturating_sub(n(li, field)).clamp(0, extent)
        });
        let atlas_extent = [i64::from(atlas.width()), i64::from(atlas.height())];
        let alpha = num(def, "displayOpacity", 1.);
        let ox = n(li, "__pxTotalOffsetX");
        let oy = n(li, "__pxTotalOffsetY");
        let values = if classic {
            arr(&li["gridTiles"])
        } else {
            arr(&li["autoLayerTiles"])
        };
        let grid = n(li, "__gridSize").max(1) as f64;
        let pivot_x = num(def, "tilePivotX", 0.);
        let pivot_y = num(def, "tilePivotY", 0.);
        let offsets = |grid_pivot: f64, tile_pivot: f64| {
            [false, true].map(|flipped| {
                let grid_offset = (grid_pivot + if flipped { 1. } else { 0. }) * grid;
                let tile_offset = match (classic, flipped) {
                    (true, false) => tile_pivot * size as f64,
                    (true, true) => (1. - tile_pivot) * size as f64,
                    (false, false) => 0.,
                    (false, true) => size as f64,
                };
                (grid_offset - tile_offset).round() as i64
            })
        };
        let x_offsets = offsets(pivot_x, pivot_x);
        // Classic tiles use the X pivot for the grid offset on both axes.
        let y_offsets = offsets(if classic { pivot_x } else { pivot_y }, pivot_y);
        let mut opaque = HashMap::new();
        for t in values {
            let f = n(t, "f");
            let flip_x = f & 1 != 0;
            let flip_y = f & 2 != 0;
            let px = &t["px"];
            let position = |axis: usize, offset: i64, pivot: i64| {
                let pixel = i128::from(px[axis].as_i64().unwrap_or(0));
                i64::try_from(pixel + i128::from(offset) + i128::from(pivot)).ok()
            };
            let (Some(tx), Some(ty)) = (
                position(0, ox, x_offsets[usize::from(flip_x)]),
                position(1, oy, y_offsets[usize::from(flip_y)]),
            ) else {
                continue;
            };
            let src = &t["src"];
            let sx = src[0].as_i64().unwrap_or(0);
            let sy = src[1].as_i64().unwrap_or(0);
            let tile_alpha = alpha * num(t, "a", 1.);
            if tile_alpha >= 1.
                && tile_fits(clip, [tx, ty], size)
                && *opaque.entry([sx, sy]).or_insert_with(|| {
                    tile_fits(atlas_extent, [sx, sy], size)
                        && opaque_atlas_tile(atlas, [sx, sy], size)
                })
            {
                copy_opaque_tile(&mut img, atlas, [tx, ty], [sx, sy], size, f);
                continue;
            }
            let x_range = visible_tile_pixels(tx, clip[0], sx, atlas_extent[0], size, flip_x);
            let y_range = visible_tile_pixels(ty, clip[1], sy, atlas_extent[1], size, flip_y);
            if x_range.is_empty() || y_range.is_empty() {
                continue;
            }
            for y in y_range {
                let ay = sy + if flip_y { size - 1 - y } else { y };
                for x in x_range.clone() {
                    let ax = sx + if flip_x { size - 1 - x } else { x };
                    blend_rgba(
                        img.get_pixel_mut((tx + x) as u32, (ty + y) as u32),
                        *atlas.get_pixel(ax as u32, ay as u32),
                        tile_alpha,
                    );
                }
            }
        }
        Ok((img, false))
    }
}
fn intgrid_image(layer: &Value, def: &Value) -> Result<RgbaImage> {
    let (columns, rows) = (n(layer, "__cWid"), n(layer, "__cHei"));
    let (width, height) = checked_image_dimensions(columns, rows).ok_or_else(|| {
        Error::Invalid(format!(
            "invalid IntGrid export image dimensions for layer {}: {columns} × {rows}",
            s(layer, "__identifier")
        ))
    })?;
    let mut colors = std::collections::HashMap::new();
    for entry in arr(&def["intGridValues"]) {
        colors
            .entry(n(entry, "value"))
            .or_insert_with(|| color(s(entry, "color")));
    }
    let cells = arr(&layer["intGridCsv"]);
    let mut image = RgbaImage::new(width, height);
    for (index, pixel) in image.pixels_mut().enumerate() {
        let value = cells.get(index).and_then(Value::as_i64).unwrap_or(0);
        if let Some(color) = colors.get(&value) {
            *pixel = *color;
        }
    }
    Ok(image)
}

fn dimensions(l: &Value) -> Result<(u32, u32)> {
    checked_image_dimensions(n(l, "pxWid"), n(l, "pxHei"))
        .ok_or_else(|| Error::Invalid("invalid export image dimensions".into()))
}

pub(super) fn checked_image_dimensions(width: i64, height: i64) -> Option<(u32, u32)> {
    if width <= 0 || height <= 0 || width.checked_mul(height)? > 268_435_456 {
        return None;
    }
    Some((width as u32, height as u32))
}
