use super::{ExportBatch, arr, bytes, n, s, safe_name};
use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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
            batch.add(dir.join(format!("{name}.yy")), &bytes(&room)?)?;
            progress(index + 1, total);
        }
        if total > 0 {
            progress(0, 0);
        }
        batch.publish()
    }

    fn gamemaker_room(&self, level: &Value) -> Result<Value> {
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
            if kind == "Tiles" {
                let tileset = instance["__tilesetDefUid"]
                    .as_i64()
                    .and_then(|uid| self.tileset(uid))
                    .ok_or_else(|| Error::Invalid("GMS tile layer has no tileset".into()))?;
                layer["tiles"] = gamemaker_tiles(instance);
                layer["x"] = json!(n(instance, "__pxTotalOffsetX"));
                layer["y"] = json!(n(instance, "__pxTotalOffsetY"));
                layer["tilesetId"] = json!({
                    "name":tileset["identifier"], "path":format!("{}.yy",s(tileset,"identifier"))
                });
            }
            layers.push(layer);
        }
        let mut room = json!({
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
        // Move the tile arrays into their original field position without cloning them.
        room["layers"] = Value::Array(layers);
        Ok(room)
    }
}

fn gamemaker_tiles(instance: &Value) -> Value {
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
    json!({"SerialiseWidth":width,"SerialiseHeight":height,"TileSerialiseData":tiles})
}
