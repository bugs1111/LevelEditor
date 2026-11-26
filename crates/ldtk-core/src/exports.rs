//! Filesystem exporters ported from the pinned LDtk exporters and ProjectSaver.
mod gamemaker;
mod png;
mod tiled;

use png::PngRenderer;
pub use png::{
    PngExportMode, PngExportOptions, background_image_layout, classic_tile_cell,
    unavailable_classic_tile_red,
};
pub use tiled::TiledFormat;

use crate::project::{FilesToKeep, StagedSave, StagingWriter, absolute};
use crate::{Error, Project, Result};
use image::{GenericImage, Rgba, RgbaImage, codecs::png::PngEncoder};
use indexmap::{IndexMap, IndexSet};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::{self, BufReader, BufWriter, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};

static CSV_BYTE_TEXT: LazyLock<[String; 256]> =
    LazyLock::new(|| std::array::from_fn(|byte| format!("{byte},")));
#[derive(Clone, Debug, Default)]
pub struct ExportReport {
    pub outputs: Vec<PathBuf>,
    pub warnings: Vec<String>,
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
struct ExportBatch<'a> {
    project: &'a Project,
    seen: IndexSet<PathBuf>,
    staged: StagedSave,
}

impl<'a> ExportBatch<'a> {
    fn new(project: &'a Project) -> Self {
        Self {
            project,
            seen: IndexSet::new(),
            staged: StagedSave::for_exports(),
        }
    }

    fn add(&mut self, path: PathBuf, bytes: &[u8]) -> Result<()> {
        self.add_parts(path, &[bytes])
    }

    fn add_parts(&mut self, path: PathBuf, parts: &[&[u8]]) -> Result<()> {
        self.prepare_target(&path)?;
        self.staged.add_parts(path, parts, true)
    }

    fn add_writer(
        &mut self,
        path: PathBuf,
        write: impl FnOnce(&mut BufWriter<StagingWriter<'_>>) -> io::Result<()>,
    ) -> Result<()> {
        self.prepare_target(&path)?;
        self.staged.add_writer(path, write)
    }

    fn add_png(&mut self, path: PathBuf, image: &RgbaImage) -> Result<()> {
        self.prepare_target(&path)?;
        let result = self
            .staged
            .add_writer(path.clone(), |writer| write_png(image, writer));
        if let Err(Error::Io { source, .. }) = &result
            && matches!(
                source.kind(),
                io::ErrorKind::PermissionDenied
                    | io::ErrorKind::ReadOnlyFilesystem
                    | io::ErrorKind::StorageFull
                    | io::ErrorKind::QuotaExceeded
            )
        {
            // An identical output can still be reused when there is no room or permission to stage.
            let mut bytes = Vec::new();
            write_png(image, &mut bytes).map_err(|error| Error::Invalid(error.to_string()))?;
            return self.staged.add_parts(path, &[&bytes], true);
        }
        result
    }

    fn prepare_target(&mut self, path: &Path) -> Result<()> {
        if !self.seen.insert(path.to_owned()) {
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
        Ok(())
    }

    fn publish(self) -> Result<Vec<PathBuf>> {
        let mut referenced = None;
        for path in self.staged.targets() {
            match fs::symlink_metadata(path) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(Error::Io {
                        path: path.to_owned(),
                        source,
                    });
                }
            }
            let files = match &referenced {
                Some(files) => files,
                None => referenced.insert(FilesToKeep::new(self.project.referenced_file_paths()?)),
            };
            if files.contains(&absolute(path)?) {
                return Err(Error::Invalid(format!(
                    "export would overwrite a file referenced by the project: {}",
                    path.display()
                )));
            }
        }
        self.staged.publish()?;
        Ok(self.seen.into_iter().collect())
    }
}

fn bytes(v: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(v).map_err(|e| Error::Invalid(e.to_string()))
}
fn fields(v: &Value) -> IndexMap<&str, &Value> {
    arr(&v["fieldInstances"])
        .iter()
        .map(|f| (s(f, "__identifier"), &f["__value"]))
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SimplifiedLevel<'a> {
    identifier: &'a Value,
    #[serde(rename = "uniqueIdentifer")]
    unique_identifier: &'a Value,
    x: &'a Value,
    y: &'a Value,
    width: &'a Value,
    height: &'a Value,
    bg_color: &'a Value,
    neighbour_levels: &'a Value,
    custom_fields: IndexMap<&'a str, &'a Value>,
    layers: Vec<String>,
    entities: IndexMap<&'a str, Vec<SimplifiedEntity<'a>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SimplifiedEntity<'a> {
    id: &'a Value,
    iid: &'a Value,
    layer: &'a Value,
    x: &'a Value,
    y: &'a Value,
    width: &'a Value,
    height: &'a Value,
    color: u32,
    custom_fields: IndexMap<&'a str, &'a Value>,
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
fn write_png(img: &RgbaImage, writer: impl Write) -> io::Result<()> {
    // The image encoder writes IEND in Drop, which can discard a write error.
    struct PngOutput<W> {
        writer: W,
        error: Option<io::Error>,
    }

    impl<W: Write> Write for PngOutput<W> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.error.is_none() {
                self.error = self.writer.write_all(bytes).err();
            }
            match &self.error {
                Some(error) => Err(error.kind().into()),
                None => Ok(bytes.len()),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            self.writer.flush()
        }
    }

    let mut output = PngOutput {
        writer,
        error: None,
    };
    let encoded = img
        .write_with_encoder(PngEncoder::new(&mut output))
        .map_err(|error| match error {
            image::ImageError::IoError(source) => source,
            other => io::Error::other(other),
        });
    output.error.map_or(encoded, Err)
}

#[cfg(test)]
#[test]
fn png_write_reports_footer_io_failure() {
    struct RejectEndChunk;

    impl Write for RejectEndChunk {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes == b"IEND" {
                return Err(io::Error::other("cannot write PNG footer"));
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let error = write_png(&RgbaImage::new(1, 1), RejectEndChunk).unwrap_err();
    assert_eq!(error.to_string(), "cannot write PNG footer");
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
        let mut batch = ExportBatch::new(self);
        self.stage_csv(dir.as_ref(), &mut batch, &mut progress)?;
        batch.publish()
    }

    fn stage_csv(
        &self,
        dir: &Path,
        batch: &mut ExportBatch<'_>,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<()> {
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        let byte_text = &*CSV_BYTE_TEXT;
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
                batch.add_writer(
                    dir.join(name)
                        .join(format!("{}.csv", safe_name(s(li, "__identifier"))?)),
                    |writer| {
                        let mut row = String::new();
                        let mut values = arr(&li["intGridCsv"])
                            .iter()
                            .map(|value| value.as_i64().unwrap_or(0) as u8)
                            .chain(std::iter::repeat(0));
                        for y in 0..h {
                            row.clear();
                            for value in values.by_ref().take(w) {
                                row.push_str(&byte_text[usize::from(value)]);
                            }
                            if y + 1 == h {
                                row.pop();
                            } else if w == 0 {
                                row.push(',');
                            }
                            row.push('\n');
                            writer.write_all(row.as_bytes())?;
                        }
                        Ok(())
                    },
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
        let mut batch = ExportBatch::new(self);
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
            let mut entities = IndexMap::<_, Vec<_>>::new();
            let mut layers = Vec::new();
            for li in arr(&l["layerInstances"]).iter().rev() {
                let typ = s(li, "__type");
                if typ == "Tiles"
                    || typ == "AutoLayer"
                    || (typ == "IntGrid" && !li["__tilesetDefUid"].is_null())
                {
                    layers.push(format!("{}.png", s(li, "__identifier")));
                }
            }
            for li in arr(&l["layerInstances"]) {
                for e in arr(&li["entityInstances"]) {
                    let color =
                        u32::from_str_radix(s(e, "__smartColor").trim_start_matches('#'), 16)
                            .unwrap_or(0);
                    entities
                        .entry(s(e, "__identifier"))
                        .or_default()
                        .push(SimplifiedEntity {
                            id: &e["__identifier"],
                            iid: &e["iid"],
                            layer: &li["__identifier"],
                            x: &e["px"][0],
                            y: &e["px"][1],
                            width: &e["width"],
                            height: &e["height"],
                            color,
                            custom_fields: fields(e),
                        });
                }
            }
            let simple = SimplifiedLevel {
                identifier: &l["identifier"],
                unique_identifier: &l["iid"],
                x: &l["worldX"],
                y: &l["worldY"],
                width: &l["pxWid"],
                height: &l["pxHei"],
                bg_color: &l["__bgColor"],
                neighbour_levels: &l["__neighbours"],
                custom_fields: fields(l),
                layers,
                entities,
            };
            batch.add_writer(
                dir.join(safe_name(s(l, "identifier"))?).join("data.json"),
                |writer| serde_json::to_writer_pretty(writer, &simple).map_err(io::Error::from),
            )?;
            progress(index + 1, total);
        }
        self.stage_csv(dir, &mut batch, |done, phase_total| {
            if phase_total > 0 && done > 0 {
                progress(count + done, total);
            }
        })?;
        let options = PngExportOptions {
            mode: PngExportMode::LayersAndLevels,
            include_background: self.json["exportLevelBg"].as_bool().unwrap_or(true),
            file_pattern: Some("%layer_name".into()),
        };
        let mut renderer = PngRenderer::new(self);
        for (index, key) in keys.into_iter().enumerate() {
            let l = self.level(key).unwrap();
            renderer.stage_level_png(
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
            .map(image::DynamicImage::into_rgba8)
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
            .copy_from(
                &frame_image(index)?,
                index % side * width,
                index / side * height,
            )
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
