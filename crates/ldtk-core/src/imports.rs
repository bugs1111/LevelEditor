//! Ogmo 3.3+ importer, preserving upstream's grid/rotation limitations.
use crate::exports::load_image_rgba;
use crate::project::{absolute, read_json};
use crate::workflow::{MAX_LAYER_CELLS, layer_cell_extent};
use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone)]
pub struct OgmoImportReport {
    pub project: Project,
    pub warnings: Vec<String>,
}
fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn n(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn iid() -> Value {
    json!(uuid::Uuid::new_v4().to_string())
}
fn uid(p: &mut Project) -> i64 {
    let id = p.json["nextUid"].as_i64().unwrap_or(1);
    p.json["nextUid"] = json!(id + 1);
    id
}
pub(crate) fn clean_identifier(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let leading = out.bytes().take_while(u8::is_ascii_digit).count();
    if leading > 0 {
        out = format!("_{}", out[leading..].trim_start_matches('_'));
    }
    if out.is_empty() {
        out.push('_');
    }
    out
}
fn rgb(raw: &str) -> String {
    format!(
        "#{}",
        raw.trim_start_matches('#').get(..6).unwrap_or("000000")
    )
}
fn grid(v: &Value, fallback: i64) -> Result<i64> {
    if v["x"].is_null() && v["y"].is_null() {
        return Ok(fallback);
    }
    v["x"]
        .as_i64()
        .filter(|x| (1..=i64::from(i32::MAX)).contains(x) && v["y"].as_i64() == Some(*x))
        .ok_or_else(|| Error::Unsupported("invalid or rectangular Ogmo grid cells".into()))
}

fn atlas_metric(tileset: &Value, fields: [&str; 2]) -> Result<i64> {
    let read = |field: &str| {
        let value = &tileset[field];
        if value.is_null() {
            return Ok(None);
        }
        value
            .as_i64()
            .filter(|n| (0..=i64::from(i32::MAX)).contains(n))
            .map(Some)
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "invalid {field} in Ogmo tileset {}",
                    s(tileset, "label")
                ))
            })
    };
    let [x, y] = fields.map(read);
    let (x, y) = (x?, y?);
    if x.zip(y).is_some_and(|(x, y)| x != y) {
        return Err(Error::Unsupported(format!(
            "{} and {} differ in Ogmo tileset {}",
            fields[0],
            fields[1],
            s(tileset, "label")
        )));
    }
    Ok(x.or(y).unwrap_or(0))
}

enum OgmoCells<'a> {
    Flat(&'a [Value], usize),
    Rows(&'a [Value]),
}

impl<'a> OgmoCells<'a> {
    fn new(layer: &'a Value, flat: &str, rows: &str, width: usize) -> Self {
        match layer[flat].as_array() {
            Some(cells) => Self::Flat(cells, width),
            None => Self::Rows(arr(&layer[rows])),
        }
    }

    fn cells(&self) -> impl Iterator<Item = ([usize; 2], &'a Value)> {
        let (flat, width, rows) = match *self {
            Self::Flat(cells, width) => (cells, width, &[][..]),
            Self::Rows(rows) => (&[][..], 1, rows),
        };
        let flat = flat
            .iter()
            .enumerate()
            .map(move |(i, cell)| ([i % width, i / width], cell));
        let rows = rows.iter().enumerate().flat_map(|(y, row)| {
            arr(row)
                .iter()
                .enumerate()
                .map(move |(x, cell)| ([x, y], cell))
        });
        flat.chain(rows)
    }

    fn get(&self, x: usize, y: usize) -> Option<&'a Value> {
        match *self {
            Self::Flat(cells, width) if x < width => {
                cells.get(y.checked_mul(width)?.checked_add(x)?)
            }
            Self::Flat(..) => None,
            Self::Rows(rows) => arr(rows.get(y)?).get(x),
        }
    }
}
fn walk(dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.into(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.into(),
            source,
        })?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if ft.is_dir() {
            walk(&path, out)?;
        } else if ft.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            out.insert(path);
        }
    }
    Ok(())
}
fn value_wrapper(kind: &str, value: &Value, depth: usize) -> Value {
    let (id, mut raw) = match kind {
        "Int" => ("V_Int", value.clone()),
        "Float" => ("V_Float", value.clone()),
        "Bool" => ("V_Bool", value.clone()),
        "Color" => (
            "V_Int",
            json!(value.as_i64().unwrap_or_else(|| {
                i64::from_str_radix(
                    value.as_str().unwrap_or("#000000").trim_start_matches('#'),
                    16,
                )
                .unwrap_or(0)
            })),
        ),
        _ => ("V_String", value.clone()),
    };
    if let Some(s) = raw.as_str() {
        let mut s = s.to_owned();
        for _ in 0..depth {
            s = crate::ldtk_string::escape(&s);
        }
        raw = json!(s);
    }
    json!({"id":id,"params":[raw]})
}
fn field_def(p: &mut Project, e: &Value, value: &Value) -> Result<Value> {
    let id = uid(p);
    let identifier = clean_identifier(s(value, "name"));
    let (internal, external) = match s(value, "definition") {
        "Integer" => (json!("F_Int"), "Int".into()),
        "Float" => (json!("F_Float"), "Float".into()),
        "Boolean" => (json!("F_Bool"), "Bool".into()),
        "Color" => (json!("F_Color"), "Color".into()),
        "String" => (json!("F_String"), "String".into()),
        "Text" => (json!("F_Text"), "Multilines".into()),
        "Filepath" => (json!("F_Path"), "FilePath".into()),
        "Enum" => {
            let enum_uid = uid(p);
            let name = clean_identifier(&format!("{}_{}", s(e, "name"), s(value, "name")));
            let vals:Vec<_>=arr(&value["choices"]).iter().map(|v|json!({"id":clean_identifier(v.as_str().unwrap_or("")),"tileRect":null,"color":null})).collect();
            p.json["defs"]["enums"].as_array_mut().unwrap().push(json!({"uid":enum_uid,"identifier":name,"externalRelPath":null,"externalFileChecksum":null,"iconTilesetUid":null,"values":vals}));
            (
                json!(format!("F_Enum({enum_uid})")),
                format!("LocalEnum.{name}"),
            )
        }
        other => return Err(Error::Unsupported(format!("Ogmo field type {other}"))),
    };
    let default = if external == "Color" {
        json!(rgb(value["defaults"].as_str().unwrap_or("#000000")))
    } else {
        value["defaults"].clone()
    };
    let wrapper =
        if default.is_null() || external.starts_with("LocalEnum.") || external == "FilePath" {
            Value::Null
        } else {
            value_wrapper(&external, &default, 1)
        };
    Ok(
        json!({"uid":id,"identifier":identifier,"__type":external,"type":internal,"isArray":false,"canBeNull":external.starts_with("LocalEnum.")||external=="FilePath"||external=="String"||external=="Multilines","defaultOverride":wrapper,"min":if value["bounded"]==true{value["min"].clone()}else{Value::Null},"max":if value["bounded"]==true{value["max"].clone()}else{Value::Null},"acceptFileTypes":value["extensions"],"editorDisplayMode":if value["display"]==1{"ValueOnly"}else{"NameAndValue"},"editorDisplayPos":"Above","editorLinkStyle":"ZigZag","editorDisplayColor":null,"editorAlwaysShow":false,"editorShowInWorld":true,"editorCutLongValues":true,"editorIsSearchable":false,"exportToToc":false,"doc":null,"arrayMinLength":null,"arrayMaxLength":null,"textLanguageMode":null,"symmetricalRef":false,"autoChainRef":true,"allowOutOfLevelRef":true,"allowedRefs":"Any","allowedRefsEntityUid":null,"allowedRefTags":[],"tilesetUid":null}),
    )
}
impl Project {
    pub fn import_ogmo(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::import_ogmo_with_report(path)?.project)
    }
    pub fn import_ogmo_with_report(path: impl AsRef<Path>) -> Result<OgmoImportReport> {
        let mut warnings = Vec::new();
        let path = absolute(path.as_ref())?;
        let source = read_json(&path)?;
        let version: Vec<u32> = s(&source, "ogmoVersion")
            .split('.')
            .map(|v| v.parse().unwrap_or(0))
            .collect();
        if version.first().copied().unwrap_or(0) < 3
            || (version[0] == 3 && version.get(1).copied().unwrap_or(0) < 3)
        {
            return Err(Error::Unsupported(
                "Ogmo projects must be saved with version 3.3 or later".into(),
            ));
        }
        let base = path.parent().expect("absolute project path");
        let mut p = Project::new();
        p.path = Some(path.with_extension("ldtk"));
        p.json["identifierStyle"] = json!("Free");
        p.json["worldLayout"] = json!("Free");
        p.json["bgColor"] = json!(rgb(s(&source, "backgroundColor")));
        p.json["defaultLevelBgColor"] = p.json["bgColor"].clone();
        p.json["defaultGridSize"] = json!(grid(&source["layerGridDefaultSize"], 16)?);
        let mut tilesets = BTreeMap::new();
        for ts in arr(&source["tilesets"]) {
            let label = s(ts, "label");
            let size =
                grid(&json!({"x":ts["tileWidth"],"y":ts["tileHeight"]}), 16).map_err(|_| {
                    Error::Unsupported(format!(
                        "invalid or rectangular grid in Ogmo tileset {label}"
                    ))
                })?;
            let spacing = atlas_metric(ts, ["tileSeparationX", "tileSeparationY"])?;
            let padding = atlas_metric(ts, ["tileMarginX", "tileMarginY"])?;
            let stride = size + spacing;
            let rel = s(ts, "path");
            let atlas_path = base.join(rel);
            let (dimensions, rel_path) = match load_image_rgba(&atlas_path) {
                Ok(image) => (Some(image.dimensions()), Some(rel)),
                Err(error) => match atlas_path.try_exists() {
                    Ok(false) => {
                        warnings.push(format!(
                            "File not found for Ogmo tileset {}: {}",
                            s(ts, "label"),
                            atlas_path.display()
                        ));
                        (None, None)
                    }
                    Ok(true) => {
                        warnings.push(format!(
                            "Image loading failed for Ogmo tileset {}: {error}",
                            s(ts, "label")
                        ));
                        (None, Some(rel))
                    }
                    Err(_) => return Err(error),
                },
            };
            let id = uid(&mut p);
            let (width, height, cols, rows) = if let Some((width, height)) = dimensions {
                let cols = (i64::from(width) - 2 * padding + spacing) / stride;
                let rows = (i64::from(height) - 2 * padding + spacing) / stride;
                if cols <= 0 || rows <= 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
                    return Err(Error::Invalid(format!(
                        "invalid image dimensions for Ogmo tileset {label}: {width} × {height}"
                    )));
                }
                (width, height, cols, rows)
            } else {
                (0, 0, 0, 0)
            };
            p.json["defs"]["tilesets"].as_array_mut().unwrap().push(json!({"uid":id,"identifier":clean_identifier(s(ts,"label")),"relPath":rel_path,"embedAtlas":null,"pxWid":width,"pxHei":height,"tileGridSize":size,"spacing":spacing,"padding":padding,"__cWid":cols,"__cHei":rows,"tags":[],"enumTags":[],"customData":[],"savedSelections":[],"cachedPixelData":null,"tagsSourceEnumUid":null}));
            tilesets.insert(s(ts, "label").to_owned(), id);
        }
        for e in arr(&source["entities"]) {
            let name = clean_identifier(s(e, "name"));
            let width = n(&e["size"], "x").max(1);
            let height = n(&e["size"], "y").max(1);
            let id = p.add_entity_def(&name, width as i32, height as i32)?;
            let mut fields = Vec::new();
            for value in arr(&e["values"]) {
                fields.push(field_def(&mut p, e, value)?);
            }
            if e["hasNodes"] == true {
                let id = uid(&mut p);
                fields.push(json!({"uid":id,"identifier":"ogmoNodes_array","__type":"Array<Point>","type":"F_Point","isArray":true,"canBeNull":false,"defaultOverride":null}));
            }
            let ed = p.json["defs"]["entities"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|d| d["uid"] == id)
                .unwrap();
            ed["color"] = json!(rgb(s(e, "color")));
            ed["pivotX"] = json!((n(&e["origin"], "x") as f64 / width as f64 / 0.5).round() * 0.5);
            ed["pivotY"] = json!((n(&e["origin"], "y") as f64 / height as f64 / 0.5).round() * 0.5);
            ed["maxCount"] = json!(n(e, "limit").max(0));
            ed["resizableX"] = json!(e["resizeableX"] == true);
            ed["resizableY"] = json!(e["resizeableY"] == true);
            ed["tags"] = if e["tags"].is_array() {
                e["tags"].clone()
            } else {
                json!([])
            };
            ed["fieldDefs"] = json!(fields);
        }
        let mut legends: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
        for layer in arr(&source["layers"]).iter().rev() {
            let kind = match s(layer, "definition") {
                "grid" => "IntGrid",
                "tile" => "Tiles",
                "entity" => "Entities",
                other => return Err(Error::Unsupported(format!("Ogmo layer type {other}"))),
            };
            let id = p.add_layer(
                &clean_identifier(s(layer, "name")),
                kind,
                grid(&layer["gridSize"], 16)? as i32,
            )?;
            let ld = p.json["defs"]["layers"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|d| d["uid"] == id)
                .unwrap();
            if kind == "IntGrid" {
                let mut values = Vec::new();
                let mut map = BTreeMap::new();
                if let Some(legend) = layer["legend"].as_object() {
                    // Ogmo's JavaScript object enumerates index keys before other keys.
                    let mut entries: Vec<_> = legend.iter().collect();
                    entries.sort_by_key(|(key, _)| {
                        key.parse::<u32>()
                            .ok()
                            .filter(|_| {
                                (key.len() == 1 || !key.starts_with('0'))
                                    && key.bytes().all(|c| c.is_ascii_digit())
                            })
                            .unwrap_or(u32::MAX)
                    });
                    let mut identifiers = BTreeSet::new();
                    for (i, (key, color)) in entries.into_iter().enumerate() {
                        map.insert(key.clone(), i as i64);
                        if i == 0 {
                            continue;
                        }
                        let base = if key.bytes().all(|c| c.is_ascii_digit()) {
                            format!("v{key}")
                        } else {
                            key.clone()
                        };
                        let mut identifier = clean_identifier(&base);
                        let mut suffix = 2;
                        while !identifiers.insert(identifier.clone()) {
                            identifier = clean_identifier(&format!("{base}_{suffix}"));
                            suffix += 1;
                        }
                        values.push(json!({
                            "value":i,"identifier":identifier,"color":rgb(color.as_str().unwrap_or("")),
                            "tile":null,"groupUid":0
                        }));
                    }
                }
                ld["intGridValues"] = json!(values);
                legends.insert(id, map);
            }
            if kind == "Tiles" {
                ld["tilesetDefUid"] = layer["defaultTileset"]
                    .as_str()
                    .and_then(|s| tilesets.get(s))
                    .map(|id| json!(id))
                    .unwrap_or(Value::Null);
            }
            for name in ["requiredTags", "excludedTags"] {
                if layer[name].is_array() {
                    ld[name] = layer[name].clone();
                }
            }
        }
        let template = p.json["levels"][0].clone();
        p.grid_presence.clear();
        let defs = arr(&p.json["defs"]["layers"]).to_vec();
        let mut files = BTreeSet::new();
        for dir in arr(&source["levelPaths"]) {
            if let Some(dir) = dir.as_str() {
                walk(&base.join(dir), &mut files)?;
            }
        }
        let mut levels = Vec::new();
        let mut rows: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
        for file in files {
            let input = match read_json(&file) {
                Ok(v)
                    if v["width"].is_number()
                        && v["height"].is_number()
                        && v["layers"].is_array() =>
                {
                    v
                }
                _ => continue,
            };
            let [Some(width), Some(height)] = ["width", "height"].map(|field| {
                input[field]
                    .as_i64()
                    .filter(|n| (1..=i64::from(i32::MAX)).contains(n))
            }) else {
                return Err(Error::Invalid(format!(
                    "invalid Ogmo level dimensions in {}",
                    file.display()
                )));
            };
            let mut level = template.clone();
            let level_uid = uid(&mut p);
            level["uid"] = json!(level_uid);
            level["iid"] = iid();
            level["identifier"] = json!(clean_identifier(
                file.file_stem().and_then(|s| s.to_str()).unwrap_or("Level")
            ));
            level["useAutoIdentifier"] = json!(false);
            level["pxWid"] = input["width"].clone();
            level["pxHei"] = input["height"].clone();
            level["__bgColor"] = p.json["defaultLevelBgColor"].clone();
            rows.entry(file.parent().unwrap_or(base).to_path_buf())
                .or_default()
                .push(levels.len());
            let mut instances = Vec::new();
            for d in &defs {
                let grid = n(d, "gridSize");
                let data = arr(&input["layers"])
                    .iter()
                    .find(|l| clean_identifier(s(l, "name")) == s(d, "identifier"))
                    .unwrap_or(&Value::Null);
                let [Some(ox), Some(oy)] = ["offsetX", "offsetY"].map(|field| {
                    n(&input, field)
                        .checked_add(n(data, field))
                        .and_then(|n| i32::try_from(n).ok())
                        .map(i64::from)
                }) else {
                    return Err(Error::Invalid(format!(
                        "invalid Ogmo layer offset in {}",
                        file.display()
                    )));
                };
                let cw = layer_cell_extent(width, ox, grid);
                let ch = layer_cell_extent(height, oy, grid);
                if cw > MAX_LAYER_CELLS || ch > MAX_LAYER_CELLS || cw * ch > MAX_LAYER_CELLS {
                    return Err(Error::Invalid(format!(
                        "Ogmo layer dimensions exceed the cell limit in {}",
                        file.display()
                    )));
                }
                let (cw, ch) = (cw as usize, ch as usize);
                // The incoming row stride is independent of the offset-adjusted layer width.
                let source_width = data["gridCellsX"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|&n| n > 0)
                    .unwrap_or_else(|| layer_cell_extent(width, 0, grid) as usize);
                let mut li = json!({"__identifier":d["identifier"],"__type":d["type"],"__cWid":cw,"__cHei":ch,"__gridSize":grid,"__opacity":d["displayOpacity"],"__pxTotalOffsetX":ox,"__pxTotalOffsetY":oy,"__tilesetDefUid":null,"__tilesetRelPath":null,"iid":iid(),"levelId":level_uid,"layerDefUid":d["uid"],"pxOffsetX":ox,"pxOffsetY":oy,"visible":true,"optionalRules":[],"intGridCsv":[],"autoLayerTiles":[],"seed":0,"overrideTilesetUid":null,"gridTiles":[],"entityInstances":[]});
                match s(d, "type") {
                    "IntGrid" => {
                        let map = legends.get(&n(d, "uid")).unwrap();
                        let input_values = OgmoCells::new(data, "grid", "grid2D", source_width);
                        let mut csv = vec![0i64; cw * ch];
                        let mut presence = crate::grid_presence::GridPresence::empty();
                        for ([x, y], v) in input_values.cells() {
                            if x >= cw || y >= ch {
                                continue;
                            }
                            let i = x + y * cw;
                            let key = v
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string());
                            if !map.contains_key(&key) {
                                warnings.push(format!(
                                    "Unknown IntGrid value {key} in {}: exported as empty",
                                    s(d, "identifier")
                                ));
                            }
                            csv[i] = map.get(&key).copied().unwrap_or(0);
                            if csv[i] == 0 {
                                presence.set(i, true);
                            }
                        }
                        li["intGridCsv"] = json!(csv);
                        let iid = li["iid"].as_str().expect("imported layer IID");
                        p.grid_presence.insert(iid.into(), presence);
                    }
                    "Tiles" => {
                        let id = data["tileset"]
                            .as_str()
                            .and_then(|s| tilesets.get(s))
                            .copied()
                            .or(d["tilesetDefUid"].as_i64());
                        if let Some(id) = id {
                            let td = p.tileset(id).unwrap();
                            li["__tilesetDefUid"] = json!(id);
                            li["__tilesetRelPath"] = td["relPath"].clone();
                            if d["tilesetDefUid"] != id {
                                li["overrideTilesetUid"] = json!(id);
                            }
                            let flags =
                                OgmoCells::new(data, "tileFlags", "tileFlags2D", source_width);
                            let rotations = flags
                                .cells()
                                .filter(|(_, f)| f.as_i64().is_some_and(|f| f & 1 != 0))
                                .count();
                            if rotations > 0 {
                                warnings.push(format!("Found {rotations} unsupported Ogmo tile rotations in {}: rotations ignored",s(d,"identifier")));
                            }
                            let cols = n(td, "__cWid");
                            let tile_count = cols * n(td, "__cHei");
                            let stride = n(td, "tileGridSize") + n(td, "spacing");
                            let padding = n(td, "padding");
                            let mut tiles = Vec::new();
                            let mut outside = None;
                            let input_tiles = OgmoCells::new(data, "data", "data2D", source_width);
                            for ([x, y], t) in input_tiles.cells() {
                                if t.is_null() || x >= cw || y >= ch {
                                    continue;
                                }
                                let invalid_tile = || {
                                    Error::Invalid(format!(
                                        "invalid Ogmo tile ID or atlas coordinates at ({x}, {y}) in layer {} of {}",
                                        s(d, "identifier"),
                                        file.display()
                                    ))
                                };
                                let tid = t
                                    .as_i64()
                                    .filter(|&id| id <= i64::from(i32::MAX))
                                    .ok_or_else(invalid_tile)?;
                                if tid < 0 {
                                    continue;
                                }
                                let f = flags.get(x, y).and_then(Value::as_i64).unwrap_or(0);
                                let flips = ((f >> 2) & 1) | (f & 2);
                                let row = if cols == 0 { 0 } else { tid / cols };
                                let col = tid - row * cols;
                                let [Ok(src_x), Ok(src_y)] =
                                    [col, row].map(|axis| i32::try_from(padding + axis * stride))
                                else {
                                    return Err(invalid_tile());
                                };
                                if tile_count > 0 && tid >= tile_count {
                                    outside.get_or_insert((0, tid)).0 += 1;
                                }
                                tiles.push(json!({"px":[x as i64*grid,y as i64*grid],"src":[src_x,src_y],"f":flips,"t":tid,"d":[x+y*cw],"a":1}));
                            }
                            if let Some((count, first)) = outside {
                                warnings.push(format!(
                                    "Found {count} Ogmo tiles outside tileset {} in layer {} of {} (first tile ID {first}); kept for repair",
                                    s(td, "identifier"), s(d, "identifier"), file.display()
                                ));
                            }
                            li["gridTiles"] = json!(tiles);
                        }
                    }
                    "Entities" => {
                        let mut entities = Vec::new();
                        for e in arr(&data["entities"]) {
                            let name = s(e, "name");
                            let identifier = clean_identifier(name);
                            let Some(ed) = arr(&p.json["defs"]["entities"])
                                .iter()
                                .find(|d| d["identifier"] == identifier)
                            else {
                                warnings.push(format!(
                                    "Unknown Ogmo entity {name} in {}",
                                    file.display()
                                ));
                                continue;
                            };
                            let mut fields = Vec::new();
                            for fd in arr(&ed["fieldDefs"]) {
                                let typ = s(fd, "__type");
                                let explicit = e["values"]
                                    .as_object()
                                    .and_then(|values| {
                                        values.iter().find(|(key, _)| {
                                            clean_identifier(key) == s(fd, "identifier")
                                        })
                                    })
                                    .map(|(_, v)| v.clone());
                                let value = explicit.clone().unwrap_or(Value::Null);
                                let value = if typ == "Color" {
                                    json!(rgb(value.as_str().unwrap_or("")))
                                } else if typ == "FilePath" {
                                    value
                                        .as_str()
                                        .and_then(|s| s.split_once(':').map(|(_, p)| json!(p)))
                                        .unwrap_or(Value::Null)
                                } else if typ.starts_with("LocalEnum.") {
                                    value
                                        .as_str()
                                        .map(|s| json!(clean_identifier(s)))
                                        .unwrap_or(Value::Null)
                                } else if typ == "Array<Point>" {
                                    json!([])
                                } else {
                                    value
                                };
                                let wrapper = if value.is_null()
                                    || typ == "Array<Point>"
                                    || explicit.is_none()
                                {
                                    json!([])
                                } else {
                                    json!([value_wrapper(typ, &value, 2)])
                                };
                                fields.push(json!({"__identifier":fd["identifier"],"__type":fd["__type"],"__value":value,"__tile":null,"defUid":fd["uid"],"realEditorValues":wrapper}));
                            }
                            let width = if ed["resizableX"] == true {
                                e["width"].as_i64().unwrap_or(n(ed, "width"))
                            } else {
                                n(ed, "width")
                            };
                            let height = if ed["resizableY"] == true {
                                e["height"].as_i64().unwrap_or(n(ed, "height"))
                            } else {
                                n(ed, "height")
                            };
                            entities.push(json!({"__identifier":ed["identifier"],"__grid":[n(e,"x")/grid,n(e,"y")/grid],"__pivot":[ed["pivotX"],ed["pivotY"]],"__tags":ed["tags"],"__tile":null,"__smartColor":ed["color"],"iid":iid(),"defUid":ed["uid"],"px":[n(e,"x"),n(e,"y")],"width":width,"height":height,"fieldInstances":fields}));
                        }
                        li["entityInstances"] = json!(entities);
                    }
                    _ => (),
                }
                instances.push(li);
            }
            level["layerInstances"] = json!(instances);
            levels.push(level);
        }
        let mut y = 0;
        for row in rows.values() {
            let mut x = 0;
            let mut height = 0;
            for index in row {
                let level = &mut levels[*index];
                level["worldX"] = json!(x);
                level["worldY"] = json!(y);
                x += n(level, "pxWid") + 16;
                height = height.max(n(level, "pxHei"));
            }
            y += height + 64;
        }
        p.json["levels"] = json!(levels);
        p.refresh_derived_data()?;
        Ok(OgmoImportReport {
            project: p,
            warnings,
        })
    }
}
