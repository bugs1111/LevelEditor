use super::{ExportBatch, ExportReport, arr, bytes, color, n, num, encode_png, s, safe_name};
use crate::project::relative_to;
use crate::{Error, Project, Result};
use image::RgbaImage;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::path::{Path, PathBuf};

fn xml(v: impl std::fmt::Display) -> String {
    v.to_string()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn property(name: &str, typ: Option<&str>, value: &Value) -> String {
    format!(
        "<property name=\"{}\"{} value=\"{}\"/>",
        xml(name),
        typ.map(|t| format!(" type=\"{t}\"")).unwrap_or_default(),
        xml(match value {
            Value::String(s) => s.clone(),
            Value::Null => "null".into(),
            v => v.to_string(),
        })
    )
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
            outputs: self.export_tiled_with_progress(dir, &mut progress)?,
            warnings,
        })
    }
    fn tiled_tilesets(
        &self,
        dir: &Path,
        batch: &mut ExportBatch,
    ) -> Result<(String, BTreeMap<i64, u32>)> {
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));
        let mut tilesets = String::new();
        let mut gids = BTreeMap::new();
        let mut gid = 1u32;
        for td in arr(&self.json["defs"]["tilesets"]) {
            let Some(rel) = td["relPath"].as_str() else {
                continue;
            };
            let size = n(td, "tileGridSize").max(1);
            let cols = (n(td, "pxWid") + size - 1) / size;
            let count = cols * ((n(td, "pxHei") + size - 1) / size);
            let source = relative_to(&base.join(rel), dir)?;
            gids.insert(n(td, "uid"), gid);
            tilesets.push_str(&format!("<tileset firstgid=\"{gid}\" name=\"{}\" tilewidth=\"{size}\" tileheight=\"{size}\" tilecount=\"{count}\" columns=\"{cols}\" objectalignment=\"topleft\" margin=\"0\" spacing=\"{}\"><image source=\"{}\" width=\"{}\" height=\"{}\"/>",xml(s(td,"identifier")),n(td,"spacing"),xml(source),n(td,"pxWid"),n(td,"pxHei")));
            for data in arr(&td["customData"]) {
                let raw = s(data, "data").replace("\\\\", "\\").replace("\\n", "\n");
                let value = serde_json::from_str::<Value>(&raw).map_err(|e| {
                    Error::Invalid(format!(
                        "invalid Tiled custom data for tile {}: {e}",
                        n(data, "tileId")
                    ))
                })?;
                if let Value::Object(values) = value {
                    let props: String = values
                        .into_iter()
                        .filter(|(_, v)| !v.is_array() && !v.is_object())
                        .map(|(k, v)| {
                            property(
                                &k,
                                if v.is_boolean() {
                                    Some("bool")
                                } else if v.is_i64() {
                                    Some("int")
                                } else if v.is_number() {
                                    Some("float")
                                } else {
                                    None
                                },
                                &v,
                            )
                        })
                        .collect();
                    if !props.is_empty() {
                        tilesets.push_str(&format!(
                            "<tile id=\"{}\"><properties>{props}</properties></tile>",
                            n(data, "tileId")
                        ));
                    }
                }
            }
            tilesets.push_str("</tileset>");
            gid = gid.saturating_add(count as u32);
        }
        for d in arr(&self.json["defs"]["layers"])
            .iter()
            .filter(|d| s(d, "type") == "IntGrid")
        {
            let values = arr(&d["intGridValues"]);
            let size = n(d, "gridSize").max(1) as u32;
            let width = size * values.len() as u32;
            let filename = format!("{}.intgrid.png", safe_name(s(d, "identifier"))?);
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
                batch.add(path, &encode_png(&image)?)?;
            }
            gids.insert(n(d, "uid"), gid);
            tilesets.push_str(&format!("<tileset firstgid=\"{gid}\" name=\"{}\" tilewidth=\"{size}\" tileheight=\"{size}\" tilecount=\"{}\" columns=\"{}\" objectalignment=\"topleft\"><image source=\"{}\" width=\"{}\" height=\"{size}\"/></tileset>",xml(s(d,"identifier")),values.len(),values.len(),xml(filename),width));
            gid += values.len() as u32;
        }
        Ok((tilesets, gids))
    }

    /// TMX deliberately exports the first world only, matching upstream's limitation.
    pub fn export_tiled(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_tiled_with_progress(dir, &mut |_, _| {})
    }
    fn export_tiled_with_progress(
        &self,
        dir: impl AsRef<Path>,
        progress: &mut impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut batch = ExportBatch::default();
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
            let (tilesets, gids) = self.tiled_tilesets(dir, &mut batch)?;
            for (index, level) in levels.enumerate() {
                let (l, [x, y]) =
                    level.ok_or_else(|| Error::Invalid("missing external level".into()))?;
                let xml = self.tiled_map_xml(l, grid, &tilesets, &gids);
                let filename = if total > 1 {
                    format!("{:04}_{}.tmx", index + 1, safe_name(s(l, "identifier"))?)
                } else {
                    format!("{}.tmx", safe_name(s(l, "identifier"))?)
                };
                batch.add(dir.join(&filename), xml.as_bytes())?;
                maps.push(json!({"fileName":filename,"x":x,"y":y}));
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
        l: &Value,
        grid: i64,
        tilesets: &str,
        gids: &BTreeMap<i64, u32>,
    ) -> String {
        let mut layers = String::new();
        let mut layer_id = 1;
        let object_id = 1;
        for li in arr(&l["layerInstances"]).iter().rev() {
            let def = self.layer_def(n(li, "layerDefUid")).unwrap_or(li);
            if n(li, "__gridSize") != grid {
                continue;
            }
            let cw = n(li, "__cWid").max(0) as usize;
            let ch = n(li, "__cHei").max(0) as usize;
            let typ = s(li, "__type");
            let name = s(li, "__identifier");
            let opacity = num(def, "displayOpacity", 1.);
            if typ == "Entities" {
                layers.push_str(&format!(
                    "<objectgroup id=\"{layer_id}\" name=\"{}\">",
                    xml(name)
                ));
                layer_id += 1;
                for e in arr(&li["entityInstances"]) {
                    let ed = self.definition("entities", n(e, "defUid")).unwrap_or(e);
                    let x = e["px"][0].as_i64().unwrap_or(0);
                    let y = e["px"][1].as_i64().unwrap_or(0);
                    let width = n(ed, "width");
                    let height = n(ed, "height");
                    let ex = x - (num(ed, "pivotX", 0.) * width as f64).round() as i64;
                    let ey = y - (num(ed, "pivotY", 0.) * height as f64).round() as i64;
                    layers.push_str(&format!("<object name=\"{}\" type=\"{}\" x=\"{ex}\" y=\"{ey}\" width=\"{width}\" height=\"{height}\"><properties>",xml(s(e,"__identifier")),xml(s(e,"__identifier"))));
                    for (k, v) in [
                        ("__anchorX", x),
                        ("__anchorY", y),
                        ("__cx", (x - i64::from(num(ed, "pivotX", 0.) == 1.)) / grid),
                        ("__cy", (y - i64::from(num(ed, "pivotY", 0.) == 1.)) / grid),
                    ] {
                        layers.push_str(&property(k, Some("int"), &json!(v)));
                    }
                    for f in arr(&e["fieldInstances"]) {
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
                        let values = if let Some(a) = f["__value"].as_array() {
                            a.clone()
                        } else {
                            vec![f["__value"].clone()]
                        };
                        for (i, v) in values.iter().enumerate() {
                            let name = if values.len() > 1 {
                                format!("{}_{i}", s(f, "__identifier"))
                            } else {
                                s(f, "__identifier").to_owned()
                            };
                            let v = match raw_type {
                                "Color" => json!(format!(
                                    "#ff{}",
                                    v.as_str().unwrap_or("#000000").trim_start_matches('#')
                                )),
                                "EntityRef" => v["entityIid"].clone(),
                                "Point" if !v.is_null() => {
                                    json!(format!("{},{}", n(v, "cx"), n(v, "cy")))
                                }
                                "Tile" if !v.is_null() => json!(format!(
                                    "{},{},{},{}",
                                    n(v, "x"),
                                    n(v, "y"),
                                    n(v, "w"),
                                    n(v, "h")
                                )),
                                "String" | "Multilines" | "FilePath" => v
                                    .as_str()
                                    .map(|v| json!(v.replace("\\\\", "\\").replace("\\n", "\n")))
                                    .unwrap_or(Value::Null),
                                _ => v.clone(),
                            };
                            layers.push_str(&property(&name, kind, &v));
                        }
                    }
                    layers.push_str("</properties></object>");
                }
                layers.push_str("</objectgroup>");
                continue;
            }
            if typ == "IntGrid" {
                let base = *gids.get(&n(li, "layerDefUid")).unwrap_or(&1);
                let cells = arr(&li["intGridCsv"]);
                let values = (0..cw * ch).map(|i| {
                    let v = cells.get(i).and_then(Value::as_i64).unwrap_or(0);
                    if v > 0 { base + v as u32 - 1 } else { 0 }
                });
                let name = if li["__tilesetDefUid"].is_number() {
                    format!("{name}_values")
                } else {
                    name.to_owned()
                };
                write_tmx_layer(&mut layers, layer_id, &name, [cw, ch], opacity, values);
                layer_id += 1;
            }
            let tiles = if typ == "Tiles" {
                arr(&li["gridTiles"])
            } else {
                arr(&li["autoLayerTiles"])
            };
            let td_uid = li["__tilesetDefUid"]
                .as_i64()
                .or_else(|| def["tilesetDefUid"].as_i64());
            if let Some(base) = td_uid.and_then(|uid| gids.get(&uid)) {
                let mut stacks: Vec<Vec<u32>> = Vec::new();
                for t in tiles {
                    let x = t["px"][0].as_i64().unwrap_or(0) / grid;
                    let y = t["px"][1].as_i64().unwrap_or(0) / grid;
                    if x < 0 || y < 0 || x >= cw as i64 || y >= ch as i64 {
                        continue;
                    }
                    let coord = x as usize + y as usize * cw;
                    let mut index = 0;
                    while index < stacks.len() && stacks[index][coord] != 0 {
                        index += 1;
                    }
                    if index == stacks.len() {
                        stacks.push(vec![0; cw * ch]);
                    }
                    let f = n(t, "f");
                    stacks[index][coord] = (*base + n(t, "t") as u32)
                        | if f & 1 != 0 { 0x80000000 } else { 0 }
                        | if f & 2 != 0 { 0x40000000 } else { 0 };
                }
                for (i, data) in stacks.iter().enumerate() {
                    let name = if stacks.len() > 1 {
                        format!("{name}_{}", i + 1)
                    } else {
                        name.to_owned()
                    };
                    write_tmx_layer(
                        &mut layers,
                        layer_id,
                        &name,
                        [cw, ch],
                        opacity,
                        data.iter().copied(),
                    );
                    layer_id += 1;
                }
            }
        }
        format!(
            "<map version=\"1.4\" tiledversion=\"1.4.2\" orientation=\"orthogonal\" renderorder=\"right-down\" compressionlevel=\"0\" width=\"{}\" height=\"{}\" tilewidth=\"{grid}\" tileheight=\"{grid}\" infinite=\"0\" backgroundcolor=\"{}\" nextlayerid=\"{layer_id}\" nextobjectid=\"{object_id}\">{tilesets}{layers}</map>",
            (n(l, "pxWid") + grid - 1) / grid,
            (n(l, "pxHei") + grid - 1) / grid,
            xml(s(l, "__bgColor"))
        )
    }
}

fn write_tmx_layer(
    out: &mut String,
    id: i32,
    name: &str,
    [w, h]: [usize; 2],
    opacity: f64,
    data: impl Iterator<Item = u32>,
) {
    write!(out, "<layer id=\"{id}\" name=\"{}\" width=\"{w}\" height=\"{h}\" opacity=\"{opacity}\"><data encoding=\"csv\">", xml(name))
        .expect("writing to a String is infallible");
    for (index, value) in data.enumerate() {
        if index > 0 {
            out.push(',');
        }
        write!(out, "{value}").expect("writing to a String is infallible");
    }
    out.push_str("</data></layer>");
}
