//! Filesystem exporters ported from the pinned LDtk exporters and ProjectSaver.
mod tiled;

use crate::project::StagedSave;
use crate::{Error, LevelKey, Project, Result};
use image::{GenericImage, Rgba, RgbaImage};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::{BufReader, Cursor, Read},
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
#[derive(Clone, Debug, Default)]
pub struct ExportReport {
    pub outputs: Vec<PathBuf>,
    pub warnings: Vec<String>,
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

fn n(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn num(v: &Value, k: &str, default: f64) -> f64 {
    v[k].as_f64().unwrap_or(default)
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn safe_name(name: &str) -> Result<&str> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        Err(Error::Invalid(format!("unsafe export name: {name}")))
    } else {
        Ok(name)
    }
}
#[derive(Default)]
struct ExportBatch {
    seen: HashSet<PathBuf>,
    staged: StagedSave,
}

impl ExportBatch {
    fn add(&mut self, path: PathBuf, bytes: &[u8]) -> Result<()> {
        if !self.seen.insert(path.clone()) {
            return Err(Error::Invalid(format!(
                "duplicate export output {}",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.into(),
                source,
            })?;
        }
        self.staged.add(path, bytes, false)
    }

    fn publish(self) -> Result<Vec<PathBuf>> {
        let paths = self.staged.targets().cloned().collect();
        self.staged.publish()?;
        Ok(paths)
    }
}

fn publish(outputs: Vec<(PathBuf, Vec<u8>)>) -> Result<Vec<PathBuf>> {
    let mut batch = ExportBatch::default();
    for (path, bytes) in outputs {
        batch.add(path, &bytes)?;
    }
    batch.publish()
}
fn bytes(v: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(v).map_err(|e| Error::Invalid(e.to_string()))
}
fn fields(v: &Value) -> Value {
    let mut o = serde_json::Map::new();
    for f in arr(&v["fieldInstances"]) {
        o.insert(s(f, "__identifier").to_owned(), f["__value"].clone());
    }
    Value::Object(o)
}
fn color(v: &str) -> Rgba<u8> {
    let raw = v.trim_start_matches('#');
    let c = u32::from_str_radix(raw, 16).unwrap_or(0);
    if raw.len() == 8 {
        Rgba([(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8])
    } else {
        Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255])
    }
}
fn png(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(out.into_inner())
}
fn blend_pixel(target: &mut RgbaImage, x: i64, y: i64, c: Rgba<u8>, alpha: f64) {
    if x < 0 || y < 0 || x >= target.width() as i64 || y >= target.height() as i64 {
        return;
    }
    let d = target.get_pixel_mut(x as u32, y as u32);
    let a = c[3] as f64 / 255.0 * alpha.clamp(0., 1.);
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
        for (x, y, p) in source.enumerate_pixels() {
            blend_pixel(target, x as i64, y as i64, *p, alpha);
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
    pub fn export_csv(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_csv_with_progress(dir, |_, _| {})
    }
    pub fn export_csv_with_progress(
        &self,
        dir: impl AsRef<Path>,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        let mut batch = ExportBatch::default();
        self.stage_csv(dir.as_ref(), &mut batch, &mut progress)?;
        batch.publish()
    }

    fn stage_csv(
        &self,
        dir: &Path,
        batch: &mut ExportBatch,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<()> {
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        for (index, key) in keys.into_iter().enumerate() {
            let l = self
                .level(key)
                .ok_or_else(|| Error::Invalid("missing external level".into()))?;
            let name = safe_name(s(l, "identifier"))?;
            for li in arr(&l["layerInstances"]) {
                if s(li, "__type") != "IntGrid" {
                    continue;
                }
                let w = n(li, "__cWid").max(0) as usize;
                let h = n(li, "__cHei").max(0) as usize;
                let mut text = String::new();
                let values = arr(&li["intGridCsv"]);
                for y in 0..h {
                    for x in 0..w {
                        if x > 0 {
                            text.push(',');
                        }
                        let value =
                            values.get(x + y * w).and_then(Value::as_i64).unwrap_or(0) as u8;
                        text.push_str(&value.to_string());
                    }
                    if y + 1 < h {
                        text.push(',');
                    }
                    text.push('\n');
                }
                batch.add(
                    dir.join(name)
                        .join(format!("{}.csv", safe_name(s(li, "__identifier"))?)),
                    text.as_bytes(),
                )?;
            }
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        Ok(())
    }
    pub fn export_simplified(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_simplified_with_progress(dir, |_, _| {})
    }
    pub fn export_simplified_with_progress(
        &self,
        dir: impl AsRef<Path>,
        progress: impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        self.export_simplified_with_diagnostics(dir, progress, &mut Vec::new())
    }
    pub(crate) fn export_simplified_with_diagnostics(
        &self,
        dir: impl AsRef<Path>,
        mut progress: impl FnMut(usize, usize),
        diagnostics: &mut Vec<String>,
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut batch = ExportBatch::default();
        let keys = self.level_keys();
        let count = keys.len();
        let total = count * 3;
        if total > 0 {
            progress(0, total);
        }
        for (index, key) in keys.iter().copied().enumerate() {
            let l = self
                .level(key)
                .ok_or_else(|| Error::Invalid("missing external level".into()))?;
            let mut entities = serde_json::Map::new();
            let mut layers = Vec::new();
            for li in arr(&l["layerInstances"]).iter().rev() {
                let typ = s(li, "__type");
                if typ == "Tiles"
                    || typ == "AutoLayer"
                    || (typ == "IntGrid" && !li["__tilesetDefUid"].is_null())
                {
                    layers.push(json!(format!("{}.png", s(li, "__identifier"))));
                }
            }
            for li in arr(&l["layerInstances"]) {
                for e in arr(&li["entityInstances"]) {
                    let entry = entities
                        .entry(s(e, "__identifier").to_owned())
                        .or_insert_with(|| json!([]));
                    entry.as_array_mut().unwrap().push(json!({"id":e["__identifier"],"iid":e["iid"],"layer":li["__identifier"],"x":e["px"][0],"y":e["px"][1],"width":e["width"],"height":e["height"],"color":u32::from_str_radix(s(e,"__smartColor").trim_start_matches('#'),16).unwrap_or(0),"customFields":fields(e)}));
                }
            }
            let simple = json!({"identifier":l["identifier"],"uniqueIdentifer":l["iid"],"x":l["worldX"],"y":l["worldY"],"width":l["pxWid"],"height":l["pxHei"],"bgColor":l["__bgColor"],"neighbourLevels":l["__neighbours"],"customFields":fields(l),"layers":layers,"entities":entities});
            batch.add(
                dir.join(safe_name(s(l, "identifier"))?).join("data.json"),
                &bytes(&simple)?,
            )?;
            progress(index + 1, total);
        }
        self.stage_csv(dir, &mut batch, |done, phase_total| {
            if phase_total > 0 && done > 0 {
                progress(count + done, total);
            }
        })?;
        for (index, key) in keys.into_iter().enumerate() {
            let l = self.level(key).unwrap();
            let options = PngExportOptions {
                mode: PngExportMode::LayersAndLevels,
                include_background: self.json["exportLevelBg"].as_bool().unwrap_or(true),
                file_pattern: Some("%layer_name".into()),
            };
            self.stage_level_png(
                key,
                &dir.join(safe_name(s(l, "identifier"))?),
                &options,
                true,
                diagnostics,
                &mut batch,
            )?;
            progress(count * 2 + index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        batch.publish()
    }
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
        let mut batch = ExportBatch::default();
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        for (index, key) in keys.into_iter().enumerate() {
            self.stage_level_png(key, dir.as_ref(), options, false, diagnostics, &mut batch)?;
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        batch.publish()
    }
    fn stage_level_png(
        &self,
        key: LevelKey,
        dir: &Path,
        options: &PngExportOptions,
        simple: bool,
        diagnostics: &mut Vec<String>,
        batch: &mut ExportBatch,
    ) -> Result<()> {
        let l = self
            .level(key)
            .ok_or_else(|| Error::Invalid("missing external level".into()))?;
        let (w, h) = dimensions(l)?;
        let name = safe_name(s(l, "identifier"))?;
        let background = if options.include_background {
            self.render_background(l, diagnostics)?
        } else {
            RgbaImage::new(w, h)
        };
        if options.include_background && options.mode != PngExportMode::OneImagePerLevel {
            batch.add(
                dir.join(if simple {
                    "_bg.png".into()
                } else {
                    format!("{name}_bg.png")
                }),
                &png(&background)?,
            )?;
        }
        let mut combined = match options.mode {
            PngExportMode::OneImagePerLayer => None,
            PngExportMode::LayersAndLevels if options.include_background => {
                Some(RgbaImage::from_pixel(w, h, self.level_background_color(l)))
            }
            _ => Some(background),
        };
        let mut premultiplied = false;
        for li in arr(&l["layerInstances"]).iter().rev() {
            let def = self.layer_def(n(li, "layerDefUid")).unwrap_or(li);
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
            let file = self.png_name(key, li, pattern)?;
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
                    batch.add(dir.join(format!("{file}.png")), &png(&image)?)?;
                }
            }
            if typ == "IntGrid" && options.mode != PngExportMode::OneImagePerLevel {
                let cw = n(li, "__cWid").max(0) as u32;
                let ch = n(li, "__cHei").max(0) as u32;
                let mut img = RgbaImage::new(cw, ch);
                let values = arr(&li["intGridCsv"]);
                for y in 0..ch {
                    for x in 0..cw {
                        let v = values
                            .get((x + y * cw) as usize)
                            .and_then(Value::as_i64)
                            .unwrap_or(0);
                        if let Some(c) = arr(&def["intGridValues"])
                            .iter()
                            .find(|c| n(c, "value") == v)
                        {
                            img.put_pixel(x, y, color(s(c, "color")));
                        }
                    }
                }
                batch.add(dir.join(format!("{file}-int.png")), &png(&img)?)?;
            }
        }
        if let Some(combined) = combined {
            let filename = if simple {
                "_composite".into()
            } else if options.mode == PngExportMode::OneImagePerLevel {
                let li = arr(&l["layerInstances"]).first().unwrap_or(l);
                self.png_name(
                    key,
                    li,
                    options.file_pattern.as_deref().unwrap_or("%level_name"),
                )?
            } else {
                name.to_owned()
            };
            batch.add(dir.join(format!("{filename}.png")), &png(&combined)?)?;
        }
        Ok(())
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
        let mut image = RgbaImage::from_pixel(w, h, self.level_background_color(l));
        if let Some(rel) = l["bgRelPath"].as_str() {
            let bg = match self.resource_image(rel) {
                Ok(bg) => bg,
                Err(error) => {
                    diagnostics.push(format!(
                        "Failed to load background {rel} for level {}: {error}",
                        s(l, "identifier")
                    ));
                    return Ok(image);
                }
            };
            let bw = bg.width() as f64;
            let bh = bg.height() as f64;
            let mode = s(l, "bgPos");
            let ([sx, sy], [cw, ch]) = background_image_layout(
                [f64::from(w), f64::from(h)],
                [bw, bh],
                mode,
            );
            let pivot_x = num(l, "bgPivotX", 0.5);
            let pivot_y = num(l, "bgPivotY", 0.5);
            let cx = pivot_x * (bw - cw);
            let cy = pivot_y * (bh - ch);
            let x = (pivot_x * (w as f64 - cw * sx)).trunc();
            let y = (pivot_y * (h as f64 - ch * sy)).trunc();
            for dy in 0..h {
                for dx in 0..w {
                    let bx = if mode == "Repeat" {
                        (dx as f64 - (w as f64 - bw) * pivot_x).rem_euclid(bw)
                    } else {
                        (dx as f64 - x) / sx + cx
                    };
                    let by = if mode == "Repeat" {
                        (dy as f64 - (h as f64 - bh) * pivot_y).rem_euclid(bh)
                    } else {
                        (dy as f64 - y) / sy + cy
                    };
                    if bx >= cx
                        && by >= cy
                        && bx < cx + cw
                        && by < cy + ch
                        && bx >= 0.
                        && by >= 0.
                        && bx < bw
                        && by < bh
                    {
                        blend_pixel(
                            &mut image,
                            dx as i64,
                            dy as i64,
                            *bg.get_pixel(bx as u32, by as u32),
                            1.,
                        );
                    }
                }
            }
        }
        Ok(image)
    }
    fn render_tile_layer(
        &self,
        l: &Value,
        li: &Value,
        placeholder_opacity: f64,
    ) -> Result<(RgbaImage, bool)> {
        let (w, h) = dimensions(l)?;
        let mut img = RgbaImage::new(w, h);
        let def = self.layer_def(n(li, "layerDefUid")).unwrap_or(li);
        let classic = s(li, "__type") == "Tiles";
        let uid = li["__tilesetDefUid"]
            .as_i64()
            .or_else(|| def["tilesetDefUid"].as_i64());
        let atlas = uid.and_then(|uid| self.tileset(uid)).map(|td| {
            let result = if let Some(rel) = td["relPath"].as_str() {
                self.resource_image(rel)
            } else if !td["embedAtlas"].is_null() {
                embedded_atlas()
            } else {
                Err(Error::Invalid("missing atlas path".into()))
            };
            (td, result)
        });
        let (td, atlas) = match atlas {
            Some((td, Ok(atlas))) => (td, atlas),
            _ if classic => {
                let painted =
                    draw_unavailable_classic_atlas(&mut img, li, def, placeholder_opacity);
                return Ok((img, painted));
            }
            None => return Ok((img, false)),
            Some((_, Err(error))) => return Err(error),
        };
        let size = n(td, "tileGridSize").max(1);
        let alpha = num(def, "displayOpacity", 1.);
        let ox = n(li, "__pxTotalOffsetX");
        let oy = n(li, "__pxTotalOffsetY");
        let values = if s(li, "__type") == "Tiles" {
            arr(&li["gridTiles"])
        } else {
            arr(&li["autoLayerTiles"])
        };
        for t in values {
            let f = n(t, "f");
            let grid = n(li, "__gridSize").max(1) as f64;
            let pivot_x = num(def, "tilePivotX", 0.);
            let pivot_y = num(def, "tilePivotY", 0.);
            let (tx, ty) = if s(li, "__type") == "Tiles" {
                (
                    t["px"][0].as_i64().unwrap_or(0)
                        + ox
                        + ((pivot_x + if f & 1 != 0 { 1. } else { 0. }) * grid
                            - if f & 1 != 0 {
                                (1. - pivot_x) * size as f64
                            } else {
                                pivot_x * size as f64
                            })
                        .round() as i64,
                    t["px"][1].as_i64().unwrap_or(0)
                        + oy
                        + ((pivot_x + if f & 2 != 0 { 1. } else { 0. }) * grid
                            - if f & 2 != 0 {
                                (1. - pivot_y) * size as f64
                            } else {
                                pivot_y * size as f64
                            })
                        .round() as i64,
                )
            } else {
                (
                    t["px"][0].as_i64().unwrap_or(0)
                        + ox
                        + ((pivot_x + if f & 1 != 0 { 1. } else { 0. }) * grid
                            - if f & 1 != 0 { size as f64 } else { 0. })
                        .round() as i64,
                    t["px"][1].as_i64().unwrap_or(0)
                        + oy
                        + ((pivot_y + if f & 2 != 0 { 1. } else { 0. }) * grid
                            - if f & 2 != 0 { size as f64 } else { 0. })
                        .round() as i64,
                )
            };
            let sx = t["src"][0].as_i64().unwrap_or(0);
            let sy = t["src"][1].as_i64().unwrap_or(0);
            let f = n(t, "f");
            for y in 0..size {
                for x in 0..size {
                    let ax = sx + if f & 1 != 0 { size - 1 - x } else { x };
                    let ay = sy + if f & 2 != 0 { size - 1 - y } else { y };
                    if ax >= 0 && ay >= 0 && ax < atlas.width() as i64 && ay < atlas.height() as i64
                    {
                        blend_pixel(
                            &mut img,
                            tx + x,
                            ty + y,
                            *atlas.get_pixel(ax as u32, ay as u32),
                            alpha * num(t, "a", 1.),
                        );
                    }
                }
            }
        }
        Ok((img, false))
    }
}
fn dimensions(l: &Value) -> Result<(u32, u32)> {
    let (w, h) = (n(l, "pxWid"), n(l, "pxHei"));
    if w <= 0 || h <= 0 || w.checked_mul(h).is_none_or(|area| area > 268_435_456) {
        return Err(Error::Invalid("invalid export image dimensions".into()));
    }
    Ok((w as u32, h as u32))
}
impl Project {
    /// This preserves upstream's experimental/incomplete GMS2 schema, including empty entity layers.
    pub fn export_gamemaker(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_gamemaker_with_progress(dir, |_, _| {})
    }
    pub fn export_gamemaker_with_progress(
        &self,
        dir: impl AsRef<Path>,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut out = Vec::new();
        for td in arr(&self.json["defs"]["tilesets"]) {
            out.push((dir.join(format!("{}.yy",safe_name(s(td,"identifier"))?)),bytes(&json!({"name":td["identifier"],"tags":[],"resourceType":"GMRTileset","resourceVersion":"1.0"}))?));
        }
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        for (index, key) in keys.into_iter().enumerate() {
            let l = self
                .level(key)
                .ok_or_else(|| Error::Invalid("missing external level".into()))?;
            let mut layers = Vec::new();
            for (i, li) in arr(&l["layerInstances"]).iter().enumerate() {
                let typ = s(li, "__type");
                let mut layer = json!({"resourceVersion":"1.0","resourceType":match typ{"IntGrid"=>"?","Entities"=>"GMRInstanceLayer",_=>"GMRTileLayer"},"name":li["__identifier"],"tags":[],"properties":[],"layers":[],"gridX":li["__gridSize"],"gridY":li["__gridSize"],"effectType":null,"effectEnabled":true,"userdefinedDepth":false,"inheritLayerDepth":false,"hierarchyFrozen":false,"inheritLayerSettings":false,"depth":i*100,"visible":true});
                if typ == "Entities" {
                    layer["instances"] = json!([]);
                }
                if typ == "Tiles" {
                    let cw = n(li, "__cWid").max(0) as usize;
                    let ch = n(li, "__cHei").max(0) as usize;
                    let grid = n(li, "__gridSize").max(1);
                    let mut tiles = vec![2147483648u32; cw * ch];
                    for t in arr(&li["gridTiles"]) {
                        let x = t["px"][0].as_i64().unwrap_or(0) / grid;
                        let y = t["px"][1].as_i64().unwrap_or(0) / grid;
                        if x >= 0 && y >= 0 && (x as usize) < cw && (y as usize) < ch {
                            let cell = &mut tiles[x as usize + y as usize * cw];
                            if *cell == 2147483648 {
                                *cell = n(t, "t") as u32;
                            }
                        }
                    }
                    layer["tiles"] =
                        json!({"SerialiseWidth":cw,"SerialiseHeight":ch,"TileSerialiseData":tiles});
                    layer["x"] = json!(n(li, "__pxTotalOffsetX"));
                    layer["y"] = json!(n(li, "__pxTotalOffsetY"));
                    let td = li["__tilesetDefUid"]
                        .as_i64()
                        .and_then(|uid| self.tileset(uid))
                        .ok_or_else(|| Error::Invalid("GMS tile layer has no tileset".into()))?;
                    layer["tilesetId"] =
                        json!({"name":td["identifier"],"path":format!("{}.yy",s(td,"identifier"))});
                }
                layers.push(layer);
            }
            let room = json!({"name":l["identifier"],"resourceType":"GMRoom","resourceVersion":"1.0","tags":[],"parent":{"name":"","path":""},"isDnd":false,"inheritCode":false,"inheritLayers":false,"inheritCreationOrder":false,"instanceCreationOrder":[],"volume":1,"creationCodeFile":"","sequenceId":null,"layers":layers,"views":[],"physicsSettings":{"inheritPhysicsSettings":false,"PhysicsWorld":false,"PhysicsWorldGravityX":0,"PhysicsWorldGravityY":10,"PhysicsWorldPixToMetres":0.1},"roomSettings":{"Width":l["pxWid"],"Height":l["pxHei"],"persistent":false,"inheritRoomSettings":false},"viewSettings":{"enableViews":false,"inheritViewSettings":false,"clearDisplayBuffer":true,"clearViewBackground":false}});
            out.push((
                dir.join(format!("{}.yy", safe_name(s(l, "identifier"))?)),
                bytes(&room)?,
            ));
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        publish(out)
    }
}
/// Decode project images. Aseprite frames are packed into LDtk's square atlas grid.
pub fn load_image_rgba(path: &Path) -> Result<RgbaImage> {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ase") || e.eq_ignore_ascii_case("aseprite"))
    {
        load_aseprite_atlas(path)
    } else {
        image::open(path)
            .map(|i| i.to_rgba8())
            .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))
    }
}

fn load_aseprite_atlas(path: &Path) -> Result<RgbaImage> {
    let io_error = |source| Error::Io {
        path: path.to_owned(),
        source,
    };
    let mut file = BufReader::new(fs::File::open(path).map_err(io_error)?);
    // LDtk uses the stored pixel dimensions regardless of Aseprite's display ratio.
    // Neutralize those two header bytes for asefile without changing the input file.
    const PIXEL_RATIO_OFFSET: usize = 34;
    let mut prefix = [0; PIXEL_RATIO_OFFSET + 2];
    file.read_exact(&mut prefix).map_err(io_error)?;
    prefix[PIXEL_RATIO_OFFSET..].fill(1);
    let ase = asefile::AsepriteFile::read(Cursor::new(prefix).chain(file))
        .map_err(|error| Error::Invalid(format!("{}: {error}", path.display())))?;
    let width = ase.width() as u32;
    let height = ase.height() as u32;
    let frames = ase.num_frames();
    if width == 0 || height == 0 || frames == 0 {
        return Err(Error::Invalid(format!(
            "{}: invalid Aseprite dimensions or frame count",
            path.display()
        )));
    }
    let frame_image = |index| {
        let frame = ase.frame(index).image();
        RgbaImage::from_raw(width, height, frame.into_raw())
            .ok_or_else(|| Error::Invalid("invalid Aseprite frame dimensions".into()))
    };
    if frames == 1 {
        return frame_image(0);
    }

    let side = (frames - 1).isqrt() + 1;
    let (atlas_width, atlas_height) = (width * side, height * side);
    image::Limits::default()
        .reserve(u64::from(atlas_width) * u64::from(atlas_height) * 4)
        .map_err(|error| Error::Invalid(format!("{}: {error}", path.display())))?;
    let mut atlas = RgbaImage::new(atlas_width, atlas_height);
    for index in 0..frames {
        atlas
            .copy_from(&frame_image(index)?, index % side * width, index / side * height)
            .map_err(|error| Error::Invalid(format!("{}: {error}", path.display())))?;
    }
    Ok(atlas)
}

pub fn embedded_atlas() -> Result<RgbaImage> {
    image::load_from_memory(include_bytes!(
        "../../../assets/embedAtlas/finalbossblues-icons_full_16.png"
    ))
    .map(|i| i.to_rgba8())
    .map_err(|e| Error::Invalid(e.to_string()))
}
