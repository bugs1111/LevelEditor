use super::{
    TileStack, TiledGids, TiledMap, TiledPaths, arr, field_text, for_each_entity_field,
    intgrid_gids, intgrid_name, n, num, s, tiled_layer_offset, tiled_tile_layers,
};
use crate::Project;
use serde::Serialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::io::{self, Write};
use std::sync::LazyLock;

use super::super::CSV_BYTE_TEXT;

fn write_layer_header(
    writer: &mut impl Write,
    mut header: Value,
    [x, y]: [i64; 2],
) -> io::Result<()> {
    if [x, y] != [0; 2] {
        header["offsetx"] = x.into();
        header["offsety"] = y.into();
    }
    let encoded = serde_json::to_vec(&header)?;
    writer.write_all(&encoded[..encoded.len() - 1])
}

fn write_tile_layer(
    writer: &mut impl Write,
    id: usize,
    name: &str,
    [width, height]: [usize; 2],
    opacity: f64,
    offset: [i64; 2],
    nonzero: impl IntoIterator<Item = (usize, u32)>,
) -> io::Result<()> {
    let header = json!({
        "id":id,"type":"tilelayer","name":name,"x":0,"y":0,"visible":true,
        "width":width,"height":height,"opacity":opacity
    });
    write_layer_header(writer, header, offset)?;
    writer.write_all(b",\"data\":[")?;
    let area = width * height;
    let mut cursor = 0;
    let numbers = &*CSV_BYTE_TEXT;
    for (coord, gid) in nonzero {
        write_empty_gids(writer, coord - cursor, true)?;
        cursor = coord + 1;
        if let Some(digits) = numbers.get(gid as usize) {
            let text = digits.as_bytes();
            writer.write_all(&text[..text.len() - usize::from(cursor == area)])?;
        } else {
            write!(writer, "{gid}")?;
            if cursor < area {
                writer.write_all(b",")?;
            }
        }
    }
    write_empty_gids(writer, area - cursor, false)?;
    writer.write_all(b"]}")
}

fn write_empty_gids(writer: &mut impl Write, mut count: usize, comma: bool) -> io::Result<()> {
    const CHUNK: usize = 4096;
    static EMPTY: LazyLock<String> = LazyLock::new(|| "0,".repeat(CHUNK));
    while count > CHUNK {
        writer.write_all(EMPTY.as_bytes())?;
        count -= CHUNK;
    }
    if count > 0 {
        writer.write_all(&EMPTY.as_bytes()[..count * 2 - usize::from(!comma)])?;
    }
    Ok(())
}

fn nonzero_gids(gids: impl Iterator<Item = u32>) -> impl Iterator<Item = (usize, u32)> {
    gids.enumerate().filter(|&(_, gid)| gid != 0)
}

fn next_layer(writer: &mut impl Write, layer_id: &mut usize) -> io::Result<usize> {
    if *layer_id > 0 {
        writer.write_all(b",")?;
    }
    *layer_id += 1;
    Ok(*layer_id)
}

#[derive(Serialize)]
pub(super) struct Property<'a> {
    name: Cow<'a, str>,
    #[serde(rename = "type")]
    kind: &'static str,
    value: PropertyValue<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum PropertyValue<'a> {
    Integer(i64),
    Scalar(&'a Value),
    Text(Cow<'a, str>),
}

pub(super) fn property<'a>(
    name: Cow<'a, str>,
    kind: Option<&'static str>,
    value: &'a Value,
    text: impl FnOnce() -> Cow<'a, str>,
) -> Property<'a> {
    use PropertyValue::{Scalar, Text};
    let (kind, value) = match kind {
        Some("int") if value.is_i64() => ("int", Scalar(value)),
        Some("float") if value.is_number() => ("float", Scalar(value)),
        Some("bool") if value.is_boolean() => ("bool", Scalar(value)),
        Some("color") => ("color", Text(text())),
        Some("file") if value.is_string() => ("file", Text(text())),
        _ => ("string", Text(text())),
    };
    Property { name, kind, value }
}

#[derive(Serialize)]
struct MapObject<'a> {
    id: usize,
    name: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    rotation: u8,
    visible: bool,
    properties: &'a [Property<'a>],
}

impl Project {
    pub(super) fn write_tiled_map_json(
        &self,
        writer: &mut impl Write,
        map: &TiledMap<'_>,
        gids: &TiledGids,
        tilesets: &[u8],
        paths: &TiledPaths<'_>,
        warn_shift: &mut dyn FnMut(&str),
    ) -> io::Result<()> {
        let grid = map.grid;
        writer.write_all(b"{\"layers\":[")?;
        let mut layer_id = 0;
        let mut object_id = 1;
        for &(li, size) in &map.layers {
            let layer_uid = n(li, "layerDefUid");
            let def = self.layer_def(layer_uid).unwrap_or(li);
            let opacity = num(def, "displayOpacity", 1.);
            let offset = tiled_layer_offset(li);
            let name = s(li, "__identifier");
            let typ = s(li, "__type");
            if typ == "Entities" {
                let id = next_layer(writer, &mut layer_id)?;
                let header = json!({
                    "id":id,"type":"objectgroup","name":name,"x":0,"y":0,
                    "visible":true,"opacity":opacity,"draworder":"topdown"
                });
                write_layer_header(writer, header, offset)?;
                writer.write_all(b",\"objects\":[")?;
                let mut properties = Vec::new();
                for (index, entity) in arr(&li["entityInstances"]).iter().enumerate() {
                    if index > 0 {
                        writer.write_all(b",")?;
                    }
                    let rect = map.entity_rect(entity);
                    let [x, y, width, height] = rect.bounds;
                    let name = s(entity, "__identifier");
                    properties.clear();
                    properties.extend(rect.anchor_properties(grid).into_iter().map(
                        |(name, value)| Property {
                            name: Cow::Borrowed(name),
                            kind: "int",
                            value: PropertyValue::Integer(value),
                        },
                    ));
                    for_each_entity_field(
                        entity,
                        paths,
                        |name, index, kind, raw_type, value, file_path| {
                            let name = index
                                .map_or(Cow::Borrowed(name), |i| Cow::Owned(format!("{name}_{i}")));
                            properties.push(property(name, kind, value, || {
                                file_path.unwrap_or_else(|| field_text(raw_type, value))
                            }));
                        },
                    )
                    .map_err(io::Error::other)?;
                    serde_json::to_writer(
                        &mut *writer,
                        &MapObject {
                            id: object_id,
                            name,
                            kind: name,
                            x,
                            y,
                            width,
                            height,
                            rotation: 0,
                            visible: true,
                            properties: &properties,
                        },
                    )?;
                    object_id += 1;
                }
                writer.write_all(b"]}")?;
                continue;
            }

            let area = size[0] * size[1];
            if typ == "IntGrid" {
                let palette = gids.intgrids.get(&layer_uid);
                let cells = arr(&li["intGridCsv"]);
                let id = next_layer(writer, &mut layer_id)?;
                write_tile_layer(
                    writer,
                    id,
                    &intgrid_name(li),
                    size,
                    opacity,
                    offset,
                    nonzero_gids(intgrid_gids(cells, area, palette)),
                )?;
            }
            for (name, stack) in tiled_tile_layers(li, def, size, grid, &gids.atlases, warn_shift) {
                let id = next_layer(writer, &mut layer_id)?;
                match stack {
                    TileStack::Dense(data) => write_tile_layer(
                        writer,
                        id,
                        &name,
                        size,
                        opacity,
                        offset,
                        nonzero_gids(data.into_iter()),
                    )?,
                    TileStack::Sparse(mut data) => {
                        data.sort_unstable_by_key(|&(coord, _)| coord);
                        write_tile_layer(writer, id, &name, size, opacity, offset, data)?;
                    }
                }
            }
        }
        let mut header = json!({
            "type":"map","version":"1.8","tiledversion":"1.8.0","orientation":"orthogonal",
            "renderorder":"right-down","infinite":false,"tilewidth":grid,"tileheight":grid,
            "width":map.size[0],"height":map.size[1],
            "nextlayerid":layer_id+1,"nextobjectid":object_id
        });
        let background = s(map.level, "__bgColor");
        if !background.is_empty() {
            header["backgroundcolor"] = json!(background);
        }
        writer.write_all(b"],\"tilesets\":[")?;
        writer.write_all(tilesets)?;
        writer.write_all(b"],")?;
        let header = serde_json::to_vec(&header)?;
        writer.write_all(&header[1..])
    }
}
