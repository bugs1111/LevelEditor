use super::{
    CSV_BYTE_TEXT, ExportBatch, ExportReport, arr, bytes, color, n, num,
    png::checked_image_dimensions, s, safe_name,
};
use crate::project::{asset_is_absolute, relative_to};
use crate::workflow::{MAX_LAYER_CELLS, layer_cell_extent};
use crate::{Error, Project, Result};
use image::RgbaImage;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

mod tmj;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TiledFormat {
    Tmx,
    Tmj,
}

impl TiledFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Tmx => "tmx",
            Self::Tmj => "tmj",
        }
    }
}

enum TiledTilesets {
    Xml(String),
    Json(Vec<Value>),
}

struct TiledPaths<'a> {
    project_dir: &'a Path,
    output_dir: &'a Path,
}

impl TiledPaths<'_> {
    fn file_property<'a>(&self, text: &'a str) -> Result<Cow<'a, str>> {
        let path = if text.contains('\\') {
            Cow::Owned(text.replace('\\', "/"))
        } else {
            Cow::Borrowed(text)
        };
        if path.is_empty() || asset_is_absolute(&path) {
            return Ok(path);
        }
        relative_to(&self.project_dir.join(path.as_ref()), self.output_dir).map(Cow::Owned)
    }
}

struct TiledMap<'a> {
    level: &'a Value,
    pixels: [i64; 2],
    size: [i64; 2],
    grid: i64,
    layers: Vec<(&'a Value, [usize; 2])>,
}

impl<'a> TiledMap<'a> {
    fn new(level: &'a Value, grid: i64) -> Result<Self> {
        let name = s(level, "identifier");
        let pixels = [n(level, "pxWid"), n(level, "pxHei")];
        if !pixels
            .iter()
            .all(|size| (1..=i64::from(i32::MAX)).contains(size))
        {
            return Err(Error::Invalid(format!(
                "invalid Tiled map dimensions for level {name}"
            )));
        }
        let size = pixels.map(|pixels| layer_cell_extent(pixels, 0, grid));
        let mut layers = Vec::new();
        for layer in arr(&level["layerInstances"])
            .iter()
            .rev()
            .filter(|layer| n(layer, "__gridSize") == grid)
        {
            let size = if layer["__type"] == "Entities" {
                [0; 2]
            } else {
                tiled_layer_size(layer).ok_or_else(|| {
                    Error::Invalid(format!(
                        "invalid Tiled layer dimensions for {} in level {name}",
                        s(layer, "__identifier")
                    ))
                })?
            };
            layers.push((layer, size));
        }
        Ok(Self {
            level,
            pixels,
            size,
            grid,
            layers,
        })
    }
}

fn tiled_layer_size(layer: &Value) -> Option<[usize; 2]> {
    let dimension = |field| {
        layer
            .get(field)
            .map_or(Some(0), Value::as_i64)
            .filter(|size| (0..=MAX_LAYER_CELLS).contains(size))
    };
    let (width, height) = (dimension("__cWid")?, dimension("__cHei")?);
    (width.checked_mul(height)? <= MAX_LAYER_CELLS).then_some([width as usize, height as usize])
}

fn tiled_layer_offset(layer: &Value) -> [i64; 2] {
    ["__pxTotalOffsetX", "__pxTotalOffsetY"].map(|field| n(layer, field))
}

#[derive(Default)]
struct TiledGids {
    atlases: BTreeMap<i64, u32>,
    intgrids: BTreeMap<i64, IntGridGids>,
}

enum IntGridGids {
    Sequential { first: u32, count: u32 },
    ByValue(HashMap<i64, u32>),
}

impl IntGridGids {
    fn new(values: &[Value], first: u32) -> Self {
        if values
            .iter()
            .enumerate()
            .all(|(i, value)| n(value, "value") == i as i64 + 1)
        {
            return Self::Sequential {
                first,
                count: values.len() as u32,
            };
        }
        let mut by_value = HashMap::with_capacity(values.len());
        for (index, entry) in values.iter().enumerate() {
            let value = n(entry, "value");
            if value > 0 {
                by_value.entry(value).or_insert(first + index as u32);
            }
        }
        Self::ByValue(by_value)
    }
}

struct Tileset<'a> {
    gid: u32,
    name: &'a str,
    size: i64,
    columns: i64,
    count: i64,
    image: &'a str,
    width: i64,
    height: i64,
    spacing: Option<i64>,
}

impl TiledTilesets {
    fn add(&mut self, set: Tileset<'_>, custom_data: &[Value]) -> Result<()> {
        let Tileset {
            gid,
            name,
            size,
            columns,
            count,
            image,
            width,
            height,
            spacing,
        } = set;
        match self {
            Self::Xml(out) => {
                write!(out, "<tileset firstgid=\"{gid}\" name=\"{}\" tilewidth=\"{size}\" tileheight=\"{size}\" tilecount=\"{count}\" columns=\"{columns}\" objectalignment=\"topleft\"", xml(name)).unwrap();
                if let Some(spacing) = spacing {
                    write!(out, " margin=\"0\" spacing=\"{spacing}\"").unwrap();
                }
                write!(
                    out,
                    "><image source=\"{}\" width=\"{width}\" height=\"{height}\"/>",
                    xml(image)
                )
                .unwrap();
                for_each_tile_property_set(custom_data, |id, values| {
                    write!(out, "<tile id=\"{id}\"><properties>").unwrap();
                    for (name, value) in values {
                        write_property(
                            out,
                            &name,
                            None,
                            scalar_kind(&value),
                            &property_text(&value),
                        );
                    }
                    out.push_str("</properties></tile>");
                })?;
                out.push_str("</tileset>");
            }
            Self::Json(out) => {
                let mut tiles = Vec::new();
                for_each_tile_property_set(custom_data, |id, values| {
                    let properties: Vec<_> = values
                        .iter()
                        .map(|(name, value)| {
                            tmj::property(Cow::Borrowed(name), scalar_kind(value), value, || {
                                property_text(value)
                            })
                        })
                        .collect();
                    tiles.push(json!({"id":id,"properties":properties}));
                })?;
                let mut set = json!({
                    "firstgid":gid,"name":name,"tilewidth":size,"tileheight":size,
                    "columns":columns,"tilecount":count,"objectalignment":"topleft",
                    "image":image,"imagewidth":width,"imageheight":height,
                    "margin":0,"spacing":spacing.unwrap_or(0)
                });
                if !tiles.is_empty() {
                    set["tiles"] = tiles.into();
                }
                out.push(set);
            }
        }
        Ok(())
    }
}

fn scalar_kind(value: &Value) -> Option<&'static str> {
    if value.is_boolean() {
        Some("bool")
    } else if value.is_i64() {
        Some("int")
    } else if value.is_number() {
        Some("float")
    } else {
        None
    }
}

fn for_each_tile_property_set(
    custom_data: &[Value],
    mut emit: impl FnMut(i64, serde_json::Map<String, Value>),
) -> Result<()> {
    for data in custom_data {
        let id = n(data, "tileId");
        let raw = s(data, "data").replace("\\\\", "\\").replace("\\n", "\n");
        let value = serde_json::from_str::<Value>(&raw)
            .map_err(|e| Error::Invalid(format!("invalid Tiled custom data for tile {id}: {e}")))?;
        if let Value::Object(mut values) = value {
            values.retain(|_, value| !value.is_array() && !value.is_object());
            if !values.is_empty() {
                emit(id, values);
            }
        }
    }
    Ok(())
}

struct XmlAttribute<'a>(&'a str);

impl fmt::Display for XmlAttribute<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_xml(out, self.0)
    }
}

fn xml(value: &str) -> XmlAttribute<'_> {
    XmlAttribute(value)
}

fn property_text(value: &Value) -> Cow<'_, str> {
    match value {
        Value::String(s) => Cow::Borrowed(s),
        Value::Null => Cow::Borrowed("null"),
        _ => Cow::Owned(value.to_string()),
    }
}

fn write_xml(out: &mut impl fmt::Write, text: &str) -> fmt::Result {
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        let entity = match byte {
            b'&' => "&amp;",
            b'"' => "&quot;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => continue,
        };
        out.write_str(&text[start..index])?;
        out.write_str(entity)?;
        start = index + 1;
    }
    out.write_str(&text[start..])
}

fn write_property(
    out: &mut String,
    name: &str,
    index: Option<usize>,
    kind: Option<&str>,
    value: &str,
) {
    out.push_str("<property name=\"");
    write_xml(out, name).expect("writing to a String is infallible");
    if let Some(index) = index {
        write!(out, "_{index}").expect("writing to a String is infallible");
    }
    out.push('"');
    if let Some(kind) = kind {
        out.push_str(" type=\"");
        out.push_str(kind);
        out.push('"');
    }
    out.push_str(" value=\"");
    write_xml(out, value).expect("writing to a String is infallible");
    out.push_str("\"/>");
}
impl Project {
    fn tiled_world_index(&self) -> Option<usize> {
        let multiworld = arr(&self.json["flags"])
            .iter()
            .any(|flag| flag == "MultiWorlds");
        let has_nested_worlds =
            arr(&self.json["levels"]).is_empty() && !arr(&self.json["worlds"]).is_empty();
        (multiworld || has_nested_worlds).then_some(0)
    }

    pub fn export_tiled_with_report(&self, dir: impl AsRef<Path>) -> Result<ExportReport> {
        self.export_tiled_with_report_progress(dir, |_, _| {})
    }
    pub fn export_tiled_with_report_progress(
        &self,
        dir: impl AsRef<Path>,
        progress: impl FnMut(usize, usize),
    ) -> Result<ExportReport> {
        self.export_tiled_format_with_report_progress(dir, TiledFormat::Tmx, progress)
    }

    pub fn export_tiled_format_with_report_progress(
        &self,
        dir: impl AsRef<Path>,
        format: TiledFormat,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<ExportReport> {
        let mut warnings = Vec::new();
        let defs = arr(&self.json["defs"]["layers"]);
        let grids: std::collections::BTreeSet<_> = defs.iter().map(|d| n(d, "gridSize")).collect();
        if grids.len() > 1 {
            warnings.push("Tiled supports one grid size per map; layers with incompatible grid size are discarded".into());
        }
        for td in arr(&self.json["defs"]["tilesets"]) {
            if !td["embedAtlas"].is_null() {
                warnings.push(format!(
                    "Embedded tileset {} is skipped by Tiled export",
                    s(td, "identifier")
                ));
            }
            if n(td, "padding") != 0 {
                warnings.push(format!(
                    "Tileset {} has padding, which is not supported by Tiled export",
                    s(td, "identifier")
                ));
            }
            if td["relPath"]
                .as_str()
                .and_then(|s| Path::new(s).extension())
                .and_then(|s| s.to_str())
                .is_some_and(|e| {
                    e.eq_ignore_ascii_case("ase") || e.eq_ignore_ascii_case("aseprite")
                })
            {
                warnings.push(format!("Aseprite atlas {} is not supported by Tiled; convert the referenced atlas to PNG",s(td,"identifier")));
            }
        }
        let first = self.tiled_world_index();
        if self.iter_level_keys().any(|k| k.world != first) {
            warnings.push(
                "Tiled export includes the first world only, matching the original exporter".into(),
            );
        }
        for key in self.iter_level_keys() {
            if let Some(l) = self.level(key) {
                for li in arr(&l["layerInstances"]) {
                    let grid = n(li, "__gridSize").max(1);
                    if arr(&li["autoLayerTiles"]).iter().any(|t| {
                        t["px"][0].as_i64().unwrap_or(0) % grid != 0
                            || t["px"][1].as_i64().unwrap_or(0) % grid != 0
                    }) {
                        warnings.push(format!("Layer {} in {} contains tiles not aligned with the grid; Tiled shifts these tiles",s(li,"__identifier"),s(l,"identifier")));
                    }
                }
            }
        }
        Ok(ExportReport {
            outputs: self.export_tiled_with_progress(dir, format, &mut progress)?,
            warnings,
        })
    }
    fn tiled_tilesets(
        &self,
        paths: &TiledPaths<'_>,
        batch: &mut ExportBatch<'_>,
        format: TiledFormat,
    ) -> Result<(TiledTilesets, TiledGids)> {
        let dir = paths.output_dir;
        let mut tilesets = match format {
            TiledFormat::Tmx => TiledTilesets::Xml(String::new()),
            TiledFormat::Tmj => TiledTilesets::Json(Vec::new()),
        };
        let mut gids = TiledGids::default();
        let mut gid = 1u32;
        for td in arr(&self.json["defs"]["tilesets"]) {
            let Some(rel) = td["relPath"].as_str() else {
                continue;
            };
            let size = n(td, "tileGridSize").max(1);
            let cols = (n(td, "pxWid") + size - 1) / size;
            let count = cols * ((n(td, "pxHei") + size - 1) / size);
            let source = relative_to(&paths.project_dir.join(rel), dir)?;
            gids.atlases.insert(n(td, "uid"), gid);
            tilesets.add(
                Tileset {
                    gid,
                    name: s(td, "identifier"),
                    size,
                    columns: cols,
                    count,
                    image: &source,
                    width: n(td, "pxWid"),
                    height: n(td, "pxHei"),
                    spacing: Some(n(td, "spacing")),
                },
                arr(&td["customData"]),
            )?;
            gid = gid.saturating_add(count as u32);
        }
        for d in arr(&self.json["defs"]["layers"])
            .iter()
            .filter(|d| s(d, "type") == "IntGrid")
        {
            let values = arr(&d["intGridValues"]);
            if values.is_empty() {
                continue;
            }
            let name = safe_name(s(d, "identifier"))?;
            let size = n(d, "gridSize").max(1);
            let (width, size) = size
                .checked_mul(values.len() as i64)
                .and_then(|width| checked_image_dimensions(width, size))
                .ok_or_else(|| {
                    Error::Invalid(format!("invalid dimensions for IntGrid tileset {name}"))
                })?;
            let count = width / size;
            let next_gid = gid
                .checked_add(count)
                .filter(|&next| next <= 0x1000_0000)
                .ok_or_else(|| {
                    Error::Invalid(format!("too many tiles for IntGrid tileset {name}"))
                })?;
            let filename = format!("{name}.intgrid.png");
            let path = dir.join(&filename);
            if !batch.seen.contains(&path) {
                let mut image = RgbaImage::new(width, size);
                for (i, v) in values.iter().enumerate() {
                    let fill = color(s(v, "color"));
                    for y in 0..size {
                        for x in 0..size {
                            image.put_pixel(i as u32 * size + x, y, fill);
                        }
                    }
                }
                batch.add_png(path, &image)?;
            }
            gids.intgrids
                .insert(n(d, "uid"), IntGridGids::new(values, gid));
            tilesets.add(
                Tileset {
                    gid,
                    name,
                    size: i64::from(size),
                    columns: i64::from(count),
                    count: i64::from(count),
                    image: &filename,
                    width: i64::from(width),
                    height: i64::from(size),
                    spacing: None,
                },
                &[],
            )?;
            gid = next_gid;
        }
        Ok((tilesets, gids))
    }

    /// TMX deliberately exports the first world only, matching upstream's limitation.
    pub fn export_tiled(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_tiled_with_progress(dir, TiledFormat::Tmx, &mut |_, _| {})
    }
    fn export_tiled_with_progress(
        &self,
        dir: impl AsRef<Path>,
        format: TiledFormat,
        progress: &mut impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut batch = ExportBatch::new(self);
        let defs = arr(&self.json["defs"]["layers"]);
        let mut sizes = defs.iter().map(|def| n(def, "gridSize"));
        let mut counts = HashMap::new();
        for size in sizes.clone() {
            *counts.entry(size).or_insert(0) += 1;
        }
        let mut grid = sizes
            .next()
            .unwrap_or_else(|| self.json["defaultGridSize"].as_i64().unwrap_or(16));
        // LDtk compares other sizes in definition order, starting with a count of one.
        let mut best_count = 1;
        for size in sizes {
            let count = counts[&size];
            if size != grid && count > best_count {
                grid = size;
                best_count = count;
            }
        }
        let grid = grid.max(1);
        let first_world = self.tiled_world_index();
        let levels = self.positioned_levels(first_world);
        let mut maps = Vec::new();
        let total = levels.len();
        if total > 0 {
            progress(0, total);
            let paths = TiledPaths {
                project_dir: self
                    .path
                    .as_deref()
                    .and_then(Path::parent)
                    .unwrap_or(Path::new(".")),
                output_dir: dir,
            };
            let (tilesets, gids) = self.tiled_tilesets(&paths, &mut batch, format)?;
            let extension = format.extension();
            for (index, level) in levels.enumerate() {
                let (l, [x, y]) =
                    level.ok_or_else(|| Error::Invalid("missing external level".into()))?;
                let name = safe_name(s(l, "identifier"))?;
                let map = TiledMap::new(l, grid)?;
                let filename = if total > 1 {
                    format!("{:04}_{name}.{extension}", index + 1)
                } else {
                    format!("{name}.{extension}")
                };
                let path = dir.join(&filename);
                let mut world_map = json!({"fileName":filename,"x":x,"y":y});
                match &tilesets {
                    TiledTilesets::Xml(tilesets) => {
                        let (header, layers) = self.tiled_map_xml(&map, &gids, &paths)?;
                        let mut parts = Vec::with_capacity(layers.len() + 3);
                        parts.extend([header.as_bytes(), tilesets.as_bytes()]);
                        for layer in &layers {
                            layer.append_parts(&mut parts);
                        }
                        parts.push(b"</map>");
                        batch.add_parts(path, &parts)?;
                    }
                    TiledTilesets::Json(tilesets) => {
                        batch.add_writer(path, |writer| {
                            self.write_tiled_map_json(writer, &map, &gids, tilesets, &paths)
                        })?;
                        world_map["width"] = map.pixels[0].into();
                        world_map["height"] = map.pixels[1].into();
                    }
                }
                maps.push(world_map);
                progress(index + 1, total);
            }
            progress(0, 0);
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        batch.add(
            dir.join(format!("{stem}.world")),
            &bytes(&json!({"maps":maps,"type":"world"}))?,
        )?;
        batch.publish()
    }
    fn tiled_map_xml(
        &self,
        map: &TiledMap<'_>,
        gids: &TiledGids,
        paths: &TiledPaths<'_>,
    ) -> Result<(String, Vec<TmxLayer>)> {
        let grid = map.grid;
        let mut layers = Vec::new();
        let mut layer_id = 1;
        let mut object_id = 1;
        for &(li, [cw, ch]) in &map.layers {
            let def = self.layer_def(n(li, "layerDefUid")).unwrap_or(li);
            let typ = s(li, "__type");
            let opacity = num(def, "displayOpacity", 1.);
            let offset = tiled_layer_offset(li);
            if typ == "Entities" {
                let mut layer = format!(
                    "<objectgroup id=\"{layer_id}\" name=\"{}\"",
                    xml(s(li, "__identifier"))
                );
                write_tmx_offset(&mut layer, offset);
                layer.push('>');
                layer_id += 1;
                for e in arr(&li["entityInstances"]) {
                    let ed = self.definition("entities", n(e, "defUid")).unwrap_or(e);
                    let rect = EntityRect::new(e, ed);
                    let [ex, ey, width, height] = rect.bounds;
                    let name = xml(s(e, "__identifier"));
                    write!(layer, "<object id=\"{object_id}\" name=\"{name}\" type=\"{name}\" x=\"{ex}\" y=\"{ey}\" width=\"{width}\" height=\"{height}\"><properties>")
                        .expect("writing to a String is infallible");
                    for (name, value) in rect.anchor_properties(grid) {
                        write_property(&mut layer, name, None, Some("int"), &value.to_string());
                    }
                    for_each_entity_field(
                        e,
                        paths,
                        |name, index, kind, raw_type, value, file_path| {
                            let text = file_path.unwrap_or_else(|| field_text(raw_type, value));
                            write_property(&mut layer, name, index, kind, &text);
                        },
                    )?;
                    layer.push_str("</properties></object>");
                    object_id += 1;
                }
                layer.push_str("</objectgroup>");
                layers.push(TmxLayer {
                    text: layer,
                    ..Default::default()
                });
                continue;
            }
            if typ == "IntGrid" {
                let palette = gids.intgrids.get(&n(li, "layerDefUid"));
                let cells = arr(&li["intGridCsv"]);
                let values = intgrid_gids(cells, cw * ch, palette);
                let name = intgrid_name(li);
                let mut layer = TmxLayer::default();
                write_tmx_layer(
                    &mut layer,
                    layer_id,
                    &name,
                    [cw, ch],
                    opacity,
                    offset,
                    values,
                );
                layers.push(layer);
                layer_id += 1;
            }
            let area = cw * ch;
            for (name, data) in tiled_tile_layers(li, def, [cw, ch], grid, &gids.atlases) {
                let mut layer = TmxLayer::default();
                match data {
                    TileStack::Dense(data) => write_tmx_layer(
                        &mut layer,
                        layer_id,
                        &name,
                        [cw, ch],
                        opacity,
                        offset,
                        data.into_iter(),
                    ),
                    TileStack::Sparse(mut data) => {
                        write_tmx_layer_start(
                            &mut layer.text,
                            layer_id,
                            &name,
                            [cw, ch],
                            opacity,
                            offset,
                        );
                        write_sparse_stack(&mut layer, &mut data, area);
                        layer.text.push_str("</data></layer>");
                    }
                }
                layers.push(layer);
                layer_id += 1;
            }
        }
        let header = format!(
            "<map version=\"1.4\" tiledversion=\"1.4.2\" orientation=\"orthogonal\" renderorder=\"right-down\" compressionlevel=\"0\" width=\"{}\" height=\"{}\" tilewidth=\"{grid}\" tileheight=\"{grid}\" infinite=\"0\" backgroundcolor=\"{}\" nextlayerid=\"{layer_id}\" nextobjectid=\"{object_id}\">",
            map.size[0],
            map.size[1],
            xml(s(map.level, "__bgColor"))
        );
        Ok((header, layers))
    }
}

struct EntityRect {
    bounds: [i64; 4],
    anchor: [i64; 2],
    pivot: [f64; 2],
}

impl EntityRect {
    fn new(entity: &Value, def: &Value) -> Self {
        let x = entity["px"][0].as_i64().unwrap_or(0);
        let y = entity["px"][1].as_i64().unwrap_or(0);
        let [width, height] = ["width", "height"].map(|field| {
            entity[field]
                .as_i64()
                .filter(|size| (1..=i64::from(i32::MAX)).contains(size))
                .unwrap_or_else(|| n(def, field))
        });
        let pivot = [num(def, "pivotX", 0.), num(def, "pivotY", 0.)];
        let left = x - (pivot[0] * width as f64).round() as i64;
        let top = y - (pivot[1] * height as f64).round() as i64;
        Self {
            bounds: [left, top, width, height],
            anchor: [x, y],
            pivot,
        }
    }

    fn anchor_properties(&self, grid: i64) -> [(&'static str, i64); 4] {
        let [x, y] = self.anchor;
        [
            ("__anchorX", x),
            ("__anchorY", y),
            ("__cx", (x - i64::from(self.pivot[0] == 1.)) / grid),
            ("__cy", (y - i64::from(self.pivot[1] == 1.)) / grid),
        ]
    }
}

fn for_each_entity_field<'a>(
    entity: &'a Value,
    paths: &TiledPaths<'_>,
    mut emit: impl FnMut(
        &'a str,
        Option<usize>,
        Option<&'static str>,
        &'a str,
        &'a Value,
        Option<Cow<'a, str>>,
    ),
) -> Result<()> {
    for f in arr(&entity["fieldInstances"]) {
        let raw_type = s(f, "__type")
            .trim_start_matches("Array<")
            .trim_end_matches('>');
        let kind = match raw_type {
            "Int" => Some("int"),
            "Float" => Some("float"),
            "Bool" => Some("bool"),
            "Color" => Some("color"),
            "FilePath" => Some("file"),
            "Tile" => Some("tile"),
            _ => None,
        };
        let value = &f["__value"];
        let values = value
            .as_array()
            .map_or_else(|| std::slice::from_ref(value), Vec::as_slice);
        let field_name = s(f, "__identifier");
        for (i, v) in values.iter().enumerate() {
            let file_path = if raw_type == "FilePath" {
                v.as_str()
                    .map(|path| paths.file_property(path))
                    .transpose()?
            } else {
                None
            };
            emit(
                field_name,
                (values.len() > 1).then_some(i),
                kind,
                raw_type,
                v,
                file_path,
            );
        }
    }
    Ok(())
}

fn field_text<'a>(raw_type: &str, v: &'a Value) -> Cow<'a, str> {
    match raw_type {
        "Color" => Cow::Owned(format!(
            "#ff{}",
            v.as_str().unwrap_or("#000000").trim_start_matches('#')
        )),
        "EntityRef" => property_text(&v["entityIid"]),
        "Point" if !v.is_null() => Cow::Owned(format!("{},{}", n(v, "cx"), n(v, "cy"))),
        "Tile" if !v.is_null() => Cow::Owned(format!(
            "{},{},{},{}",
            n(v, "x"),
            n(v, "y"),
            n(v, "w"),
            n(v, "h")
        )),
        "String" | "Multilines" => match v.as_str() {
            Some(text) if text.contains('\\') => {
                Cow::Owned(text.replace("\\\\", "\\").replace("\\n", "\n"))
            }
            Some(text) => Cow::Borrowed(text),
            None => Cow::Borrowed("null"),
        },
        "FilePath" => Cow::Borrowed("null"),
        _ => property_text(v),
    }
}

fn intgrid_name(li: &Value) -> Cow<'_, str> {
    let name = s(li, "__identifier");
    if li["__tilesetDefUid"].is_number() {
        Cow::Owned(format!("{name}_values"))
    } else {
        Cow::Borrowed(name)
    }
}

fn intgrid_gids<'a>(
    cells: &'a [Value],
    area: usize,
    palette: Option<&'a IntGridGids>,
) -> impl Iterator<Item = u32> + 'a {
    let cells = if palette.is_some() {
        &cells[..cells.len().min(area)]
    } else {
        &[]
    };
    let (first, count, by_value) = match palette {
        Some(IntGridGids::Sequential { first, count }) => (*first, u64::from(*count), None),
        Some(IntGridGids::ByValue(gids)) => (0, 0, Some(gids)),
        None => (0, 0, None),
    };
    cells
        .iter()
        .map(move |cell| {
            let value = cell.as_i64().unwrap_or(0);
            // Zero and negative cells fall outside the unsigned palette index range.
            let index = (value as u64).wrapping_sub(1);
            if index < count {
                first + index as u32
            } else {
                by_value
                    .and_then(|gids| gids.get(&value))
                    .copied()
                    .unwrap_or(0)
            }
        })
        .chain(std::iter::repeat_n(0, area - cells.len()))
}

fn tiled_tile_layers<'a>(
    li: &'a Value,
    def: &Value,
    [cw, ch]: [usize; 2],
    grid: i64,
    gids: &BTreeMap<i64, u32>,
) -> impl Iterator<Item = (Cow<'a, str>, TileStack)> + use<'a> {
    let typ = s(li, "__type");
    let name = s(li, "__identifier");
    let tiles = if typ == "Tiles" {
        arr(&li["gridTiles"])
    } else {
        arr(&li["autoLayerTiles"])
    };
    let td_uid = li["__tilesetDefUid"]
        .as_i64()
        .or_else(|| def["tilesetDefUid"].as_i64());
    let area = cw * ch;
    // Sparse ground needs both the stored cells and an index of occupied positions.
    let sparse_ground = area > 256 && tiles.len() <= area / 16;
    let (ground, stacks) = match td_uid.and_then(|uid| gids.get(&uid)).copied() {
        Some(base) if sparse_ground => collect_tiled_stacks::<true>(tiles, [cw, ch], grid, base),
        Some(base) => collect_tiled_stacks::<false>(tiles, [cw, ch], grid, base),
        None => (None, Vec::new()),
    };
    let single = stacks.is_empty();
    ground
        .into_iter()
        .chain(stacks)
        .enumerate()
        .map(move |(index, data)| {
            let name = if single {
                Cow::Borrowed(name)
            } else {
                Cow::Owned(format!("{name}_{}", index + 1))
            };
            (name, data)
        })
}

// Specialize once per layer so dense tile scans retain their direct array lookup.
fn collect_tiled_stacks<const SPARSE: bool>(
    tiles: &[Value],
    [cw, ch]: [usize; 2],
    grid: i64,
    base: u32,
) -> (Option<TileStack>, Vec<TileStack>) {
    let area = cw * ch;
    let mut dense = Vec::new();
    let mut sparse = Vec::new();
    let mut has_tiles = false;
    let mut stacks = Vec::<TileStack>::new();
    let mut next_stack = HashMap::new();
    for tile in tiles {
        let x = tile["px"][0].as_i64().unwrap_or(0) / grid;
        let y = tile["px"][1].as_i64().unwrap_or(0) / grid;
        if x < 0 || y < 0 || x >= cw as i64 || y >= ch as i64 {
            continue;
        }
        let coord = x as usize + y as usize * cw;
        let f = n(tile, "f");
        let gid = (base + n(tile, "t") as u32)
            | if f & 1 != 0 { 0x80000000 } else { 0 }
            | if f & 2 != 0 { 0x40000000 } else { 0 };
        let next = if SPARSE {
            has_tiles = true;
            match next_stack.entry(coord) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    if gid != 0 {
                        sparse.push((coord, gid));
                        entry.insert(0);
                    }
                    continue;
                }
            }
        } else {
            if dense.is_empty() {
                dense.resize(area, 0);
            }
            if dense[coord] == 0 {
                dense[coord] = gid;
                continue;
            }
            next_stack.entry(coord).or_insert(0)
        };
        let index = *next;
        *next += usize::from(gid != 0);
        if index == stacks.len() {
            stacks.push(TileStack::new(area));
        }
        stacks[index].insert(coord, gid, area);
    }
    let ground = if SPARSE {
        has_tiles.then_some(TileStack::Sparse(sparse))
    } else {
        (!dense.is_empty()).then_some(TileStack::Dense(dense))
    };
    (ground, stacks)
}

const EMPTY_TILE_BLOCK_BYTES: usize = 64 * 1024;
static EMPTY_TILE_BLOCK: LazyLock<String> =
    LazyLock::new(|| "0,".repeat(EMPTY_TILE_BLOCK_BYTES / 2));

#[derive(Default)]
struct TmxLayer {
    text: String,
    // Insertion offsets in the layer text and byte lengths of long empty runs.
    empty_runs: Vec<(usize, usize)>,
}

impl TmxLayer {
    fn push_empty_tiles(&mut self, count: usize, trailing_comma: bool) {
        if count == 0 {
            return;
        }
        let bytes = count * 2 - usize::from(!trailing_comma);
        if count >= 4096 {
            self.empty_runs.push((self.text.len(), bytes));
        } else {
            self.text.push_str(&EMPTY_TILE_BLOCK[..bytes]);
        }
    }

    fn append_parts<'a>(&'a self, parts: &mut Vec<&'a [u8]>) {
        let text = self.text.as_bytes();
        let mut start = 0;
        for &(offset, bytes) in &self.empty_runs {
            parts.push(&text[start..offset]);
            let block = EMPTY_TILE_BLOCK.as_bytes();
            parts.extend(std::iter::repeat_n(block, bytes / block.len()));
            let tail = bytes % block.len();
            if tail != 0 {
                parts.push(&block[..tail]);
            }
            start = offset;
        }
        parts.push(&text[start..]);
    }
}

fn write_tmx_layer(
    out: &mut TmxLayer,
    id: i32,
    name: &str,
    size: [usize; 2],
    opacity: f64,
    offset: [i64; 2],
    data: impl Iterator<Item = u32>,
) {
    write_tmx_layer_start(&mut out.text, id, name, size, opacity, offset);
    let start = out.text.len();
    let reserve = (size[0] * size[1] * 2).min(EMPTY_TILE_BLOCK_BYTES);
    out.text.reserve_exact(reserve + "</data></layer>".len());
    let numbers = &*CSV_BYTE_TEXT;
    let mut empty = 0;
    data.for_each(|value| {
        if value == 0 {
            empty += 1;
            return;
        }
        if empty > 0 {
            out.push_empty_tiles(std::mem::take(&mut empty), true);
        }
        if let Some(text) = numbers.get(value as usize) {
            out.text.push_str(text);
        } else {
            write!(out.text, "{value},").expect("writing to a String is infallible");
        }
    });
    if empty > 0 {
        out.push_empty_tiles(empty, false);
    } else if out.text.len() > start {
        out.text.pop();
    }
    out.text.push_str("</data></layer>");
}

fn write_tmx_layer_start(
    out: &mut String,
    id: i32,
    name: &str,
    [w, h]: [usize; 2],
    opacity: f64,
    offset: [i64; 2],
) {
    write!(
        out,
        "<layer id=\"{id}\" name=\"{}\" width=\"{w}\" height=\"{h}\" opacity=\"{opacity}\"",
        xml(name)
    )
    .expect("writing to a String is infallible");
    write_tmx_offset(out, offset);
    out.push_str("><data encoding=\"csv\">");
}

fn write_tmx_offset(out: &mut String, [x, y]: [i64; 2]) {
    if [x, y] != [0; 2] {
        write!(out, " offsetx=\"{x}\" offsety=\"{y}\"").expect("writing to a String is infallible");
    }
}

enum TileStack {
    Dense(Vec<u32>),
    Sparse(Vec<(usize, u32)>),
}

impl TileStack {
    fn new(area: usize) -> Self {
        if area <= 256 {
            Self::Dense(vec![0; area])
        } else {
            Self::Sparse(Vec::new())
        }
    }

    fn insert(&mut self, coord: usize, gid: u32, area: usize) {
        match self {
            Self::Dense(cells) => cells[coord] = gid,
            Self::Sparse(cells) => {
                if gid == 0 {
                    return;
                }
                cells.push((coord, gid));
                if cells.len() >= area.div_ceil(size_of::<(usize, u32)>() / size_of::<u32>()) {
                    let mut dense = vec![0; area];
                    for &(coord, gid) in cells.iter() {
                        dense[coord] = gid;
                    }
                    *self = Self::Dense(dense);
                }
            }
        }
    }
}

fn write_sparse_stack(out: &mut TmxLayer, cells: &mut [(usize, u32)], area: usize) {
    cells.sort_unstable_by_key(|&(coord, _)| coord);
    let mut cursor = 0;
    for &(coord, gid) in cells.iter() {
        out.push_empty_tiles(coord - cursor, true);
        write!(out.text, "{gid},").expect("writing to a String is infallible");
        cursor = coord + 1;
    }
    if cursor < area {
        out.push_empty_tiles(area - cursor, false);
    } else if area != 0 {
        out.text.pop();
    }
}
