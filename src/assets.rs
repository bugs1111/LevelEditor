use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
};

type ImageResult = (PathBuf, Result<ColorImage, String>);

pub struct Assets {
    textures: HashMap<PathBuf, TextureHandle>,
    pending: HashSet<PathBuf>,
    pub errors: HashMap<PathBuf, String>,
    tx: Sender<ImageResult>,
    rx: Receiver<ImageResult>,
}

impl Default for Assets {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            textures: HashMap::new(),
            pending: HashSet::new(),
            errors: HashMap::new(),
            tx,
            rx,
        }
    }
}

impl Assets {
    pub fn poll(&mut self, ctx: &Context) {
        while let Ok((path, result)) = self.rx.try_recv() {
            self.pending.remove(&path);
            match result {
                Ok(image) => {
                    let texture =
                        ctx.load_texture(path.to_string_lossy(), image, TextureOptions::NEAREST);
                    self.textures.insert(path, texture);
                }
                Err(error) => {
                    self.errors.insert(path, error);
                }
            }
        }
    }

    pub fn texture(&mut self, ctx: &Context, path: &Path) -> Option<TextureHandle> {
        if let Some(texture) = self.textures.get(path) {
            return Some(texture.clone());
        }
        if self.pending.contains(path) || self.errors.contains_key(path) {
            return None;
        }
        let path = path.to_path_buf();
        self.pending.insert(path.clone());
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let image = image::open(&path)
                .map(|image| {
                    let rgba = image.into_rgba8();
                    ColorImage::from_rgba_unmultiplied(
                        [rgba.width() as usize, rgba.height() as usize],
                        rgba.as_raw(),
                    )
                })
                .map_err(|e| e.to_string());
            let _ = tx.send((path, image));
            ctx.request_repaint();
        });
        None
    }
}
