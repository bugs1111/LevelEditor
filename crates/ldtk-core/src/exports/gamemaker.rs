use super::{ExportBatch, arr, bytes, n, s, safe_name};
use crate::{Error, Project, Result};
use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::{Value, json};
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

struct GameMakerTiles<'a>(&'a Value);

impl Serialize for GameMakerTiles<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        gamemaker_tiles(self.0).serialize(serializer)
    }
}

#[derive(Serialize)]
struct GameMakerTileData {
    #[serde(rename = "SerialiseWidth")]
    width: usize,
    #[serde(rename = "SerialiseHeight")]
    height: usize,
    #[serde(rename = "TileSerialiseData")]
    tiles: Vec<u32>,
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
        let mut batch = ExportBatch::default();
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
                    tiles: GameMakerTiles(instance),
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

fn gamemaker_tiles(instance: &Value) -> GameMakerTileData {
    const EMPTY: u32 = 1 << 31;
    let width = n(instance, "__cWid").max(0) as usize;
    let height = n(instance, "__cHei").max(0) as usize;
    let grid = n(instance, "__gridSize").max(1);
    let mut tiles = vec![EMPTY; width * height];
    for tile in arr(&instance["gridTiles"]) {
        let x = tile["px"][0].as_i64().unwrap_or(0) / grid;
        let y = tile["px"][1].as_i64().unwrap_or(0) / grid;
        if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
            let cell = &mut tiles[x as usize + y as usize * width];
            if *cell == EMPTY {
                *cell = n(tile, "t") as u32;
            }
        }
    }
    GameMakerTileData {
        width,
        height,
        tiles,
    }
}
