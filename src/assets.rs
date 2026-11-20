use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
};

const WORKERS: usize = 2;
const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
struct Job {
    path: PathBuf,
    generation: u64,
    ctx: Context,
}
struct Decoded {
    path: PathBuf,
    generation: u64,
    image: Result<(ColorImage, Vec<u8>), String>,
}
struct Cached {
    texture: TextureHandle,
    bytes: usize,
    last_used: u64,
    opaque: Vec<u64>,
    size: [usize; 2],
    rgba: Vec<u8>,
    averages: HashMap<[i32; 4], [u8; 4]>,
}
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub textures: usize,
    pub pending: usize,
    pub bytes: usize,
}

/// Two decoders and bounded queues keep a large project from spawning a thread
/// per atlas or accumulating unbounded decoded images while the UI is idle.
pub struct Assets {
    textures: HashMap<PathBuf, Cached>,
    generations: HashMap<PathBuf, u64>,
    pending: HashMap<PathBuf, u64>,
    pub errors: HashMap<PathBuf, String>,
    jobs: SyncSender<Job>,
    results: Receiver<Decoded>,
    clock: u64,
    bytes: usize,
    loaded_images: Vec<(PathBuf, [usize; 2])>,
}
impl Default for Assets {
    fn default() -> Self {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(64);
        let receiver = Arc::new(Mutex::new(receiver));
        let (sender, results) = mpsc::sync_channel(4);
        for _ in 0..WORKERS {
            let receiver = receiver.clone();
            let sender = sender.clone();
            std::thread::spawn(move || {
                loop {
                    let job = match receiver.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => break,
                    };
                    let Ok(job) = job else { break };
                    let image = ldtk_core::exports::load_image_rgba(&job.path)
                        .map_err(|e| e.to_string())
                        .and_then(|rgba| {
                            if rgba.width() as u64 * rgba.height() as u64
                                > (MAX_CACHE_BYTES as u64 * 8 / 65)
                            {
                                return Err("Image exceeds the 128 MiB atlas cache budget".into());
                            }
                            Ok((
                                ColorImage::from_rgba_unmultiplied(
                                    [rgba.width() as usize, rgba.height() as usize],
                                    rgba.as_raw(),
                                ),
                                rgba.into_raw(),
                            ))
                        });
                    if sender
                        .send(Decoded {
                            path: job.path,
                            generation: job.generation,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                    job.ctx.request_repaint();
                }
            });
        }
        Self {
            textures: HashMap::new(),
            generations: HashMap::new(),
            pending: HashMap::new(),
            errors: HashMap::new(),
            jobs,
            results,
            clock: 0,
            bytes: 0,
            loaded_images: Vec::new(),
        }
    }
}
impl Assets {
    pub(crate) fn loaded_image_paths(&self) -> impl Iterator<Item = &Path> {
        self.textures
            .keys()
            .map(PathBuf::as_path)
            .filter(|p| p.is_absolute())
    }

    pub(crate) fn discard_unused_file_images(&mut self, is_referenced: impl Fn(&Path) -> bool) {
        let unused = self
            .textures
            .keys()
            .chain(self.pending.keys())
            .filter(|path| path.is_absolute() && !is_referenced(path))
            .cloned()
            .collect::<Vec<_>>();
        for path in unused {
            self.invalidate(&path);
        }
    }

    pub(crate) fn store_file_image(
        &mut self,
        ctx: &Context,
        path: &Path,
        rgba: image::RgbaImage,
    ) -> ldtk_core::Result<()> {
        let size = [rgba.width() as usize, rgba.height() as usize];
        if size[0] as u64 * size[1] as u64 > MAX_CACHE_BYTES as u64 * 8 / 65 {
            return Err(ldtk_core::Error::Invalid(
                "Image exceeds the 128 MiB atlas cache budget".into(),
            ));
        }
        let image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.invalidate(path);
        let _ = self.install_pixels(ctx, path.to_owned(), image, rgba.into_raw());
        Ok(())
    }

    pub fn poll(&mut self, ctx: &Context) {
        while let Ok(result) = self.results.try_recv() {
            self.install_decoded(ctx, result);
        }
    }
    fn install_decoded(&mut self, ctx: &Context, result: Decoded) {
        if self.generations.get(&result.path).copied().unwrap_or(0) != result.generation {
            return;
        }
        self.pending.remove(&result.path);
        match result.image {
            Ok((image, rgba)) => {
                self.errors.remove(&result.path);
                self.loaded_images.push((result.path.clone(), image.size));
                let _ = self.install_pixels(ctx, result.path, image, rgba);
            }
            Err(error) => {
                self.errors.insert(result.path, error);
            }
        }
    }
    /// Successful external-image decodes since the previous drain. Stale generations never emit.
    pub fn take_loaded_images(&mut self) -> Vec<(PathBuf, [usize; 2])> {
        std::mem::take(&mut self.loaded_images)
    }
    #[cfg(test)]
    fn install(&mut self, ctx: &Context, path: PathBuf, image: ColorImage) -> TextureHandle {
        let rgba = image
            .pixels
            .iter()
            .flat_map(|p| p.to_srgba_unmultiplied())
            .collect();
        self.install_pixels(ctx, path, image, rgba)
    }
    fn install_pixels(
        &mut self,
        ctx: &Context,
        path: PathBuf,
        image: ColorImage,
        rgba: Vec<u8>,
    ) -> TextureHandle {
        let size = image.size;
        let mut opaque = vec![0u64; image.pixels.len().div_ceil(64)];
        for (i, pixel) in image.pixels.iter().enumerate() {
            if pixel.a() == 255 {
                opaque[i / 64] |= 1 << (i % 64);
            }
        }
        let bytes = image.pixels.len() * 4 + opaque.len() * 8 + rgba.len();
        self.remove_texture(&path);
        while self.bytes.saturating_add(bytes) > MAX_CACHE_BYTES && !self.textures.is_empty() {
            let oldest = self
                .textures
                .iter()
                .min_by_key(|(_, v)| v.last_used)
                .map(|(p, _)| p.clone())
                .unwrap();
            self.remove_texture(&oldest);
        }
        let texture = ctx.load_texture(path.to_string_lossy(), image, TextureOptions::NEAREST);
        self.clock += 1;
        self.bytes += bytes;
        self.textures.insert(
            path,
            Cached {
                texture: texture.clone(),
                bytes,
                last_used: self.clock,
                opaque,
                size,
                rgba,
                averages: HashMap::new(),
            },
        );
        texture
    }
    fn remove_texture(&mut self, path: &Path) {
        if let Some(old) = self.textures.remove(path) {
            self.bytes -= old.bytes;
        }
    }
    fn cached(&mut self, path: &Path) -> Option<TextureHandle> {
        let item = self.textures.get_mut(path)?;
        self.clock += 1;
        item.last_used = self.clock;
        Some(item.texture.clone())
    }
    pub fn texture(&mut self, ctx: &Context, path: &Path) -> Option<TextureHandle> {
        if let Some(texture) = self.cached(path) {
            return Some(texture);
        }
        if self.pending.contains_key(path) || self.errors.contains_key(path) {
            return None;
        }
        let generation = self.generations.get(path).copied().unwrap_or(0);
        if self
            .jobs
            .try_send(Job {
                path: path.to_owned(),
                generation,
                ctx: ctx.clone(),
            })
            .is_ok()
        {
            self.pending.insert(path.to_owned(), generation);
        }
        None
    }
    pub fn embedded(&mut self, ctx: &Context) -> Option<TextureHandle> {
        let path = Path::new("@ldtk/embedded-atlas");
        if let Some(texture) = self.cached(path) {
            return Some(texture);
        }
        let rgba = match ldtk_core::exports::embedded_atlas() {
            Ok(v) => v,
            Err(e) => {
                self.errors.insert(path.into(), e.to_string());
                return None;
            }
        };
        let image = ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        Some(self.install_pixels(ctx, path.into(), image, rgba.into_raw()))
    }
    pub fn unavailable_classic_atlas(&mut self, ctx: &Context, grid: i64) -> Option<TextureHandle> {
        let size = u32::try_from(grid).ok()?;
        let side = size as usize;
        if side == 0
            || side > ctx.input(|input| input.max_texture_side)
            || side.saturating_mul(side).saturating_mul(9) > MAX_CACHE_BYTES
        {
            return None;
        }
        let path = PathBuf::from(format!("@ldtk/unavailable-classic-atlas-{grid}"));
        if let Some(texture) = self.cached(&path) {
            return Some(texture);
        }
        let rgba = image::RgbaImage::from_fn(size, size, |x, y| {
            let red =
                ldtk_core::exports::unavailable_classic_tile_red(grid, i64::from(x), i64::from(y));
            image::Rgba([red, 0, 0, 255])
        });
        let pixels = ColorImage::from_rgba_unmultiplied([side, side], rgba.as_raw());
        Some(self.install_pixels(ctx, path, pixels, rgba.into_raw()))
    }
    pub fn builtin(&mut self, ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
        let path = PathBuf::from(format!("@ldtk/{name}"));
        if let Some(texture) = self.cached(&path) {
            return Some(texture);
        }
        match image::load_from_memory(bytes) {
            Ok(image) => {
                let image = image.into_rgba8();
                Some(self.install_pixels(
                    ctx,
                    path,
                    ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    image.into_raw(),
                ))
            }
            Err(e) => {
                self.errors.insert(path, e.to_string());
                None
            }
        }
    }
    /// Discards failed/stale generations, including results already being decoded.
    pub fn invalidate(&mut self, path: &Path) {
        *self.generations.entry(path.to_owned()).or_default() += 1;
        self.pending.remove(path);
        self.loaded_images.retain(|(loaded, _)| loaded != path);
        self.errors.remove(path);
        self.remove_texture(path);
    }
    pub fn clear(&mut self) {
        self.loaded_images.clear();
        let paths = self
            .textures
            .keys()
            .chain(self.pending.keys())
            .chain(self.errors.keys())
            .cloned()
            .collect::<Vec<_>>();
        for path in paths {
            self.invalidate(&path)
        }
    }
    pub fn tile_is_opaque(&self, path: &Path, src: [i32; 2], size: [i32; 2]) -> Option<bool> {
        let cached = self.textures.get(path)?;
        if src[0] < 0 || src[1] < 0 || size[0] <= 0 || size[1] <= 0 {
            return Some(false);
        }
        let (x, y, w, h) = (
            src[0] as usize,
            src[1] as usize,
            size[0] as usize,
            size[1] as usize,
        );
        if x.saturating_add(w) > cached.size[0] || y.saturating_add(h) > cached.size[1] {
            return Some(false);
        }
        for py in y..y + h {
            for px in x..x + w {
                let index = px + py * cached.size[0];
                if cached.opaque[index / 64] & (1 << (index % 64)) == 0 {
                    return Some(false);
                }
            }
        }
        Some(true)
    }
    /// Source TilesetDef._parseTilePixels: alpha-weighted RMS RGB, arithmetic alpha.
    /// Retained unpremultiplied pixels avoid lossy inversion of egui's texture colors.
    pub fn average_tile_color(
        &mut self,
        path: &Path,
        src: [i32; 2],
        size: [i32; 2],
    ) -> Option<[u8; 4]> {
        let key = [src[0], src[1], size[0], size[1]];
        let cached = self.textures.get_mut(path)?;
        if let Some(color) = cached.averages.get(&key) {
            return Some(*color);
        }
        if src[0] < 0
            || src[1] < 0
            || size[0] <= 0
            || size[1] <= 0
            || src[0] as usize + size[0] as usize > cached.size[0]
            || src[1] as usize + size[1] as usize > cached.size[1]
        {
            return Some([0; 4]);
        }
        let mut sum = [0f64; 4];
        for y in src[1]..src[1] + size[1] {
            for x in src[0]..src[0] + size[0] {
                let offset = (x as usize + y as usize * cached.size[0]) * 4;
                let rgba = &cached.rgba[offset..offset + 4];
                let a = rgba[3] as f64 / 255.;
                sum[3] += a;
                for channel in 0..3 {
                    let c = rgba[channel] as f64 / 255.;
                    sum[channel] += c * c * a;
                }
            }
        }
        let mut color = [0; 4];
        if sum[3] > 0. {
            for channel in 0..3 {
                color[channel] = ((sum[channel] / sum[3]).sqrt() * 255.) as u8;
            }
        }
        color[3] = (sum[3] / (size[0] as f64 * size[1] as f64) * 255.) as u8;
        // Memoization is bounded independently; tiny grids remain cheap to sample.
        if cached.averages.len() < 4096 && self.bytes + 64 <= MAX_CACHE_BYTES {
            cached.averages.insert(key, color);
            cached.bytes += 64;
            self.bytes += 64;
        }
        Some(color)
    }
    pub fn embedded_tile_is_opaque(&self, src: [i32; 2], size: [i32; 2]) -> Option<bool> {
        self.tile_is_opaque(Path::new("@ldtk/embedded-atlas"), src, size)
    }
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            textures: self.textures.len(),
            pending: self.pending.len(),
            bytes: self.bytes,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaded_image_events_report_each_successful_generation_once_and_never_stale_results() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let path = PathBuf::from("loaded.png");
        assets.pending.insert(path.clone(), 0);
        assets.install_decoded(
            &ctx,
            Decoded {
                path: path.clone(),
                generation: 0,
                image: Ok((
                    ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 255]),
                    vec![0, 0, 0, 255],
                )),
            },
        );
        assert_eq!(assets.take_loaded_images(), vec![(path.clone(), [1, 1])]);
        assert!(assets.take_loaded_images().is_empty());
        assets.invalidate(&path);
        assets.install_decoded(
            &ctx,
            Decoded {
                path: path.clone(),
                generation: 0,
                image: Ok((
                    ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 255]),
                    vec![0, 0, 0, 255],
                )),
            },
        );
        assert!(assets.take_loaded_images().is_empty());
        assets.pending.insert(path.clone(), 1);
        assets.install_decoded(
            &ctx,
            Decoded {
                path: path.clone(),
                generation: 1,
                image: Ok((
                    ColorImage::from_rgba_unmultiplied([2, 1], &[0; 8]),
                    vec![0; 8],
                )),
            },
        );
        assert_eq!(assets.take_loaded_images(), vec![(path, [2, 1])]);
    }
    #[test]
    fn average_colors_match_64_installed_original_js_vectors() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../docs/status/renderer-oracle.json")).unwrap();
        let ctx = Context::default();
        let mut assets = Assets::default();
        for (i, test) in oracle["averages"].as_array().unwrap().iter().enumerate() {
            let n = test["size"].as_u64().unwrap() as usize;
            let rgba = test["rgba"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8)
                .collect::<Vec<_>>();
            let path = PathBuf::from(format!("oracle-{i}"));
            let _ = assets.install_pixels(
                &ctx,
                path.clone(),
                ColorImage::from_rgba_unmultiplied([n, n], &rgba),
                rgba,
            );
            let expected = std::array::from_fn(|i| test["expected"][i].as_u64().unwrap() as u8);
            assert_eq!(
                assets.average_tile_color(&path, [0; 2], [n as i32; 2]),
                Some(expected),
                "vector {i}"
            );
            assert_eq!(
                assets.tile_is_opaque(&path, [0; 2], [n as i32; 2]),
                test["opaque"].as_bool()
            );
        }
    }
    #[test]
    fn embedded_original_atlas_is_cached_and_can_be_invalidated() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let texture = assets.embedded(&ctx).unwrap();
        assert!(texture.size()[0] > 0);
        let again = assets.embedded(&ctx).unwrap();
        assert_eq!(texture.id(), again.id());
        assert_eq!(assets.stats().textures, 1);
        assets.clear();
        assert_eq!(assets.stats().bytes, 0);
        assert_eq!(assets.stats().pending, 0);
    }
    #[test]
    fn preview_154_embedded_atlas_supplies_the_collision_fill() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let _texture = assets.embedded(&ctx).unwrap();
        // The preview fills the nine color swatches in atlas row 18.
        for column in 16..=24 {
            assert_eq!(
                assets.embedded_tile_is_opaque([column * 16, 288], [16; 2]),
                Some(true),
                "tile {}",
                18 * 32 + column,
            );
        }
    }
    #[test]
    fn stale_decoder_results_do_not_restore_invalidated_assets() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let path = Path::new("changed.png");
        let (tx, rx) = mpsc::sync_channel(2);
        assets.results = rx;
        assets.invalidate(path);
        tx.send(Decoded {
            path: path.into(),
            generation: 0,
            image: Ok((
                ColorImage::filled([2, 2], eframe::egui::Color32::RED),
                vec![255; 16],
            )),
        })
        .unwrap();
        assets.poll(&ctx);
        assert_eq!(assets.stats().textures, 0);
        tx.send(Decoded {
            path: path.into(),
            generation: 1,
            image: Ok((
                ColorImage::filled([2, 2], eframe::egui::Color32::GREEN),
                vec![255; 16],
            )),
        })
        .unwrap();
        assets.poll(&ctx);
        assert_eq!(assets.stats().textures, 1);
    }
    #[test]
    fn world_average_uses_original_alpha_weighted_rms_and_keeps_rgb_unpremultiplied() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let path = Path::new("average.png");
        let rgba = vec![
            255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 0, 100, 200, 50, 128,
        ];
        let _ = assets.install_pixels(
            &ctx,
            path.into(),
            ColorImage::from_rgba_unmultiplied([4, 1], &rgba),
            rgba,
        );
        assert_eq!(
            assets.average_tile_color(path, [0, 0], [2, 1]),
            Some([180, 0, 180, 255])
        );
        assert_eq!(
            assets.average_tile_color(path, [0, 0], [3, 1]),
            Some([180, 0, 180, 170])
        );
        assert_eq!(
            assets.average_tile_color(path, [3, 0], [1, 1]),
            Some([100, 200, 50, 128])
        );
        assert_eq!(
            assets.average_tile_color(path, [2, 0], [1, 1]),
            Some([0, 0, 0, 0])
        );
    }
    #[test]
    fn opacity_queries_never_treat_partial_alpha_or_outside_pixels_as_opaque() {
        let ctx = Context::default();
        let mut assets = Assets::default();
        let path = Path::new("opacity.png");
        assert_eq!(assets.tile_is_opaque(path, [0, 0], [1, 1]), None);
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 254, 0, 0, 255, 0, 255, 255, 255, 255,
        ];
        let _ = assets.install(
            &ctx,
            path.into(),
            ColorImage::from_rgba_unmultiplied([2, 2], &rgba),
        );
        assert_eq!(assets.tile_is_opaque(path, [0, 0], [1, 1]), Some(true));
        assert_eq!(assets.tile_is_opaque(path, [1, 0], [1, 1]), Some(false));
        assert_eq!(assets.tile_is_opaque(path, [0, 0], [2, 2]), Some(false));
        assert_eq!(assets.tile_is_opaque(path, [1, 1], [2, 2]), Some(false));
        assets.invalidate(path);
        assert_eq!(assets.tile_is_opaque(path, [0, 0], [1, 1]), None);
    }
}
