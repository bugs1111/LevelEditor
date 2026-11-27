use super::{ExportBatch, arr, bytes, n, s, safe_name};
use crate::workflow::MAX_LAYER_CELLS;
use crate::{Error, Project, Result};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct GameMakerRoom<'a> {
    properties: serde_json::Map<String, Value>,
    layers: Vec<GameMakerLayer<'a>>,
}

impl Serialize for GameMakerRoom<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut room = serializer.serialize_map(Some(self.properties.len()))?;
        for (key, value) in &self.properties {
            if key == "layers" {
                room.serialize_entry(key, &self.layers)?;
            } else {
                room.serialize_entry(key, value)?;
            }
        }
        room.end()
    }
}

#[derive(Serialize)]
struct GameMakerLayer<'a> {
    #[serde(flatten)]
    properties: Value,
    #[serde(flatten)]
    tile: Option<GameMakerTileLayer<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameMakerTileLayer<'a> {
    tiles: GameMakerTiles<'a>,
    x: i64,
    y: i64,
    tileset_id: Value,
}

struct GameMakerTiles<'a> {
    instance: &'a Value,
    width: usize,
    height: usize,
    grid: i64,
}

impl<'a> GameMakerTiles<'a> {
    fn new(instance: &'a Value) -> Result<Self> {
        let width = instance["__cWid"].as_i64().unwrap_or(-1);
        let height = instance["__cHei"].as_i64().unwrap_or(-1);
        let grid = n(instance, "__gridSize");
        if !(0..=MAX_LAYER_CELLS).contains(&width)
            || !(0..=MAX_LAYER_CELLS).contains(&height)
            || width * height > MAX_LAYER_CELLS
            || grid <= 0
        {
            return Err(Error::Invalid(format!(
                "invalid GameMaker tile dimensions for layer {}: {width} × {height}, grid {grid}",
                s(instance, "__identifier")
            )));
        }
        Ok(Self {
            instance,
            width: width as usize,
            height: height as usize,
            grid,
        })
    }
}

impl Serialize for GameMakerTiles<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        gamemaker_tiles(self.instance, self.width, self.height, self.grid)
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

#[derive(Serialize)]
struct GameMakerTileData {
    #[serde(rename = "SerialiseWidth")]
    width: usize,
    #[serde(rename = "SerialiseHeight")]
    height: usize,
    #[serde(rename = "TileSerialiseData")]
    tiles: GameMakerCells,
}

const EMPTY_CELL: u32 = 1 << 31;

enum GameMakerCells {
    Dense(Vec<u32>),
    Sparse {
        area: usize,
        occupied: BTreeMap<usize, u32>,
    },
}

impl Serialize for GameMakerCells {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let (area, occupied) = match self {
            Self::Dense(cells) => return cells.serialize(serializer),
            Self::Sparse { area, occupied } => (area, occupied),
        };
        let mut cells = serializer.serialize_seq(Some(*area))?;
        let mut next = 0;
        for (&index, value) in occupied {
            for _ in next..index {
                cells.serialize_element(&EMPTY_CELL)?;
            }
            cells.serialize_element(value)?;
            next = index + 1;
        }
        for _ in next..*area {
            cells.serialize_element(&EMPTY_CELL)?;
        }
        cells.end()
    }
}

impl Project {
    /// Preserves upstream's experimental GMS2 schema, including empty entity layers.
    pub fn export_gamemaker(&self, dir: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        self.export_gamemaker_with_progress(dir, |_, _| {})
    }

    pub fn export_gamemaker_with_progress(
        &self,
        dir: impl AsRef<Path>,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<Vec<PathBuf>> {
        let dir = dir.as_ref();
        let mut batch = ExportBatch::new(self);
        for tileset in arr(&self.json["defs"]["tilesets"]) {
            let name = safe_name(s(tileset, "identifier"))?;
            let resource = json!({
                "name": tileset["identifier"], "tags": [],
                "resourceType": "GMRTileset", "resourceVersion": "1.0"
            });
            batch.add(dir.join(format!("{name}.yy")), &bytes(&resource)?)?;
        }
        let keys = self.level_keys();
        let total = keys.len();
        if total > 0 {
            progress(0, total);
        }
        for (index, key) in keys.into_iter().enumerate() {
            let level = self
                .level(key)
                .ok_or_else(|| Error::Invalid("missing external level".into()))?;
            let room = self.gamemaker_room(level)?;
            let name = safe_name(s(level, "identifier"))?;
            batch.add_writer(dir.join(format!("{name}.yy")), |writer| {
                // Rooms contain a number for every cell, including empty cells.
                serde_json::to_writer(writer, &room).map_err(std::io::Error::from)
            })?;
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        batch.publish()
    }

    fn gamemaker_room<'a>(&self, level: &'a Value) -> Result<GameMakerRoom<'a>> {
        let mut layers = Vec::new();
        for (index, instance) in arr(&level["layerInstances"]).iter().enumerate() {
            let kind = s(instance, "__type");
            let resource = match kind {
                "IntGrid" => "?",
                "Entities" => "GMRInstanceLayer",
                _ => "GMRTileLayer",
            };
            let mut layer = json!({
                "resourceVersion":"1.0", "resourceType":resource, "name":instance["__identifier"],
                "tags":[], "properties":[], "layers":[], "gridX":instance["__gridSize"],
                "gridY":instance["__gridSize"], "effectType":null, "effectEnabled":true,
                "userdefinedDepth":false, "inheritLayerDepth":false, "hierarchyFrozen":false,
                "inheritLayerSettings":false, "depth":index*100, "visible":true
            });
            if kind == "Entities" {
                layer["instances"] = json!([]);
            }
            let tile = if kind == "Tiles" {
                let tileset = instance["__tilesetDefUid"]
                    .as_i64()
                    .and_then(|uid| self.tileset(uid))
                    .ok_or_else(|| Error::Invalid("GMS tile layer has no tileset".into()))?;
                Some(GameMakerTileLayer {
                    tiles: GameMakerTiles::new(instance)?,
                    x: n(instance, "__pxTotalOffsetX"),
                    y: n(instance, "__pxTotalOffsetY"),
                    tileset_id: json!({
                        "name":tileset["identifier"], "path":format!("{}.yy",s(tileset,"identifier"))
                    }),
                })
            } else {
                None
            };
            layers.push(GameMakerLayer {
                properties: layer,
                tile,
            });
        }
        let room = json!({
            "name":level["identifier"], "resourceType":"GMRoom", "resourceVersion":"1.0",
            "tags":[], "parent":{"name":"","path":""}, "isDnd":false,
            "inheritCode":false, "inheritLayers":false, "inheritCreationOrder":false,
            "instanceCreationOrder":[],
            "volume":1, "creationCodeFile":"", "sequenceId":null, "layers":[], "views":[],
            "physicsSettings":{"inheritPhysicsSettings":false,"PhysicsWorld":false,
                "PhysicsWorldGravityX":0,"PhysicsWorldGravityY":10,"PhysicsWorldPixToMetres":0.1},
            "roomSettings":{"Width":level["pxWid"],"Height":level["pxHei"],"persistent":false,"inheritRoomSettings":false},
            "viewSettings":{"enableViews":false,"inheritViewSettings":false,"clearDisplayBuffer":true,"clearViewBackground":false}
        });
        let Value::Object(properties) = room else {
            unreachable!("room is a JSON object");
        };
        Ok(GameMakerRoom { properties, layers })
    }
}

fn gamemaker_tiles(
    instance: &Value,
    width: usize,
    height: usize,
    grid: i64,
) -> Result<GameMakerTileData> {
    const TILE_INDEX_MASK: u64 = (1 << 19) - 1;
    let area = width * height;
    let grid_tiles = arr(&instance["gridTiles"]);
    // Leave room for the tree index while keeping dense layers on the array path.
    let mut tiles = if area >= 4096 && grid_tiles.len() <= area / 32 {
        GameMakerCells::Sparse {
            area,
            occupied: BTreeMap::new(),
        }
    } else {
        GameMakerCells::Dense(vec![EMPTY_CELL; area])
    };
    for tile in grid_tiles {
        let px = &tile["px"];
        let (Some(px_x), Some(px_y)) = (px[0].as_i64(), px[1].as_i64()) else {
            return Err(Error::Invalid(format!(
                "invalid GameMaker tile position in layer {}",
                s(instance, "__identifier")
            )));
        };
        let x = px_x.div_euclid(grid);
        let y = px_y.div_euclid(grid);
        if (0..width as i64).contains(&x) && (0..height as i64).contains(&y) {
            let index = x as usize + y as usize * width;
            let cell = match &mut tiles {
                GameMakerCells::Dense(cells) => &mut cells[index],
                GameMakerCells::Sparse { occupied, .. } => {
                    occupied.entry(index).or_insert(EMPTY_CELL)
                }
            };
            if *cell == EMPTY_CELL {
                let id = tile["t"]
                    .as_u64()
                    .filter(|&id| id <= TILE_INDEX_MASK)
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "invalid GameMaker tile ID at ({x}, {y}) in layer {}",
                            s(instance, "__identifier")
                        ))
                    })?;
                // GameMaker puts the horizontal mirror and vertical flip in bits 28 and 29.
                let flips = (n(tile, "f") as u32 & 3) << 28;
                *cell = id as u32 | flips;
            }
        }
    }
    Ok(GameMakerTileData {
        width,
        height,
        tiles,
    })
}
