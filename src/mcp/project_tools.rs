use ldtk_core::{FilesystemSnapshot, LevelKey, Project, TilePaint};
use serde_json::{Map, Value, json};
use std::{
    cell::OnceCell,
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};

type Arguments = Map<String, Value>;
type ToolResult = Result<Value, String>;

#[derive(serde::Serialize)]
#[serde(untagged)]
pub(super) enum ToolPayload<'a> {
    Owned(Value),
    Borrowed(&'a Value),
    Layer {
        layer: Option<&'a Value>,
    },
    Levels {
        levels: Vec<LevelSummary<'a>>,
    },
    Level {
        #[serde(flatten)]
        summary: LevelSummary<'a>,
        layers: Vec<LayerSummary<'a>>,
        field_instances: &'a Value,
    },
}

#[derive(serde::Serialize)]
pub(super) struct LevelSummary<'a> {
    iid: &'a Value,
    identifier: &'a Value,
    world_iid: Option<&'a str>,
    world_x: &'a Value,
    world_y: &'a Value,
    px_width: &'a Value,
    px_height: &'a Value,
}

#[derive(serde::Serialize)]
pub(super) struct LayerSummary<'a> {
    iid: &'a Value,
    identifier: &'a Value,
    #[serde(rename = "type")]
    kind: &'a Value,
    definition_uid: &'a Value,
    columns: &'a Value,
    rows: &'a Value,
    grid_size: &'a Value,
    tileset_uid: &'a Value,
}

pub(super) struct Session {
    path: PathBuf,
    project: Project,
    level_keys: OnceCell<HashMap<String, LevelKey>>,
    snapshot: Option<FilesystemSnapshot>,
    dirty: bool,
}

impl Session {
    pub(super) fn open(path: PathBuf) -> ldtk_core::Result<Self> {
        let (project, snapshot) = Project::load_with_snapshot(&path)?;
        Ok(Self {
            path,
            project,
            level_keys: OnceCell::new(),
            snapshot: Some(snapshot),
            dirty: false,
        })
    }

    pub(super) fn execute(
        &mut self,
        name: &str,
        args: &Arguments,
    ) -> Result<ToolPayload<'_>, String> {
        let value = match name {
            "project_info" => Ok(self.info()),
            "get_definitions" => {
                let defs = &self.project.json["defs"];
                if defs.is_object() {
                    return Ok(ToolPayload::Borrowed(defs));
                }
                Ok(json!({}))
            }
            "list_worlds" => Ok(self.list_worlds()),
            "list_levels" => {
                return self.list_levels(args.get("world_iid").and_then(Value::as_str));
            }
            "get_level" => return self.get_level(args["level_iid"].as_str().unwrap()),
            "get_layer" => {
                let (key, layer) = self.layer_key(args)?;
                return Ok(ToolPayload::Layer {
                    layer: self.project.layer(key, layer),
                });
            }
            "save_project" => self.save(),
            "reload_project" => {
                if self.dirty && args.get("discard_changes") != Some(&Value::Bool(true)) {
                    return Err("Unsaved changes. Save first or pass discard_changes=true.".into());
                }
                *self = Self::open(self.path.clone()).map_err(|error| error.to_string())?;
                Ok(self.info())
            }
            _ => self.edit(name, args),
        }?;
        Ok(ToolPayload::Owned(value))
    }

    fn info(&self) -> Value {
        json!({
            "path":self.path.to_string_lossy(),
            "iid":self.project.json["iid"],
            "json_version":self.project.json["jsonVersion"],
            "external_levels":self.project.json["externalLevels"],
            "dirty":self.dirty
        })
    }

    fn root_world_iid(&self) -> &str {
        self.project.json["dummyWorldIid"]
            .as_str()
            .unwrap_or("root")
    }

    fn world_iid(&self, world: Option<usize>) -> Option<&str> {
        match world {
            Some(index) => self.project.json["worlds"][index]["iid"].as_str(),
            None => Some(self.root_world_iid()),
        }
    }

    fn worlds(&self) -> impl Iterator<Item = (Option<usize>, &Value)> {
        let root = &self.project.json;
        let nested = root["worlds"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let include_root = nested.is_empty() || items(&root["levels"]).next().is_some();
        include_root.then_some((None, root)).into_iter().chain(
            nested
                .iter()
                .enumerate()
                .map(|(index, world)| (Some(index), world)),
        )
    }

    fn list_worlds(&self) -> Value {
        let worlds: Vec<_> = self.worlds().map(|(index, world)| {
            if index.is_some() {
                json!({"iid":world["iid"], "identifier":world["identifier"], "layout":world["worldLayout"]})
            } else {
                json!({"iid":self.root_world_iid(), "identifier":"World", "layout":world["worldLayout"]})
            }
        }).collect();
        Value::from_iter([("worlds", Value::Array(worlds))])
    }

    fn list_levels(&self, world_iid: Option<&str>) -> Result<ToolPayload<'_>, String> {
        let mut worlds = self
            .worlds()
            .filter(|(index, _)| world_iid.is_none_or(|iid| self.world_iid(*index) == Some(iid)))
            .peekable();
        if let Some(iid) = world_iid
            && worlds.peek().is_none()
        {
            return Err(format!("World IID not found: {iid}"));
        }
        let levels = worlds
            .flat_map(|(index, _)| self.project.levels_in_world(index))
            .map(|(key, level)| self.level_summary(key, level))
            .collect();
        Ok(ToolPayload::Levels { levels })
    }

    fn level_summary<'a>(&'a self, key: LevelKey, level: &'a Value) -> LevelSummary<'a> {
        LevelSummary {
            iid: &level["iid"],
            identifier: &level["identifier"],
            world_iid: self.world_iid(key.world),
            world_x: &level["worldX"],
            world_y: &level["worldY"],
            px_width: &level["pxWid"],
            px_height: &level["pxHei"],
        }
    }

    fn get_level(&self, iid: &str) -> Result<ToolPayload<'_>, String> {
        let (key, level) = self.level(iid)?;
        let layers = items(&level["layerInstances"])
            .map(|layer| LayerSummary {
                iid: &layer["iid"],
                identifier: &layer["__identifier"],
                kind: &layer["__type"],
                definition_uid: &layer["layerDefUid"],
                columns: &layer["__cWid"],
                rows: &layer["__cHei"],
                grid_size: &layer["__gridSize"],
                tileset_uid: &layer["__tilesetDefUid"],
            })
            .collect();
        Ok(ToolPayload::Level {
            summary: self.level_summary(key, level),
            layers,
            field_instances: &level["fieldInstances"],
        })
    }

    fn level(&self, iid: &str) -> Result<(LevelKey, &Value), String> {
        // The edit tools leave level IIDs and positions intact. Opening/reloading
        // a session replaces the project and its index together.
        let keys = self.level_keys.get_or_init(|| {
            let mut keys = HashMap::new();
            for key in self.project.iter_level_keys() {
                if let Some(iid) = self
                    .project
                    .level(key)
                    .and_then(|level| level["iid"].as_str())
                {
                    keys.entry(iid.to_owned()).or_insert(key);
                }
            }
            keys
        });
        keys.get(iid)
            .and_then(|&key| self.project.level(key).map(|level| (key, level)))
            .ok_or_else(|| format!("Level IID not found: {iid}"))
    }

    fn layer_key(&self, args: &Arguments) -> Result<(LevelKey, usize), String> {
        let (key, level) = self.level(args["level_iid"].as_str().unwrap())?;
        let iid = args["layer_iid"].as_str().unwrap();
        let index = items(&level["layerInstances"])
            .position(|layer| layer["iid"] == iid)
            .ok_or_else(|| format!("Layer IID not found: {iid}"))?;
        Ok((key, index))
    }

    fn edit(&mut self, name: &str, args: &Arguments) -> ToolResult {
        let (key, index) = self.layer_key(args)?;
        if matches!(
            name,
            "paint_int_grid_cells" | "paint_tile_cells" | "erase_tile_cells"
        ) {
            return self.edit_cells(name, key, index, args);
        }
        let layer = self.project.layer(key, index).expect("resolved layer");
        let coordinates = || (integer(args, "x"), integer(args, "y"));
        let mut placed_iid = None;
        let changed = match name {
            "paint_int_grid" => {
                let (x, y) = coordinates();
                in_bounds(layer, ["__cWid", "__cHei"], x, y)?;
                self.project
                    .paint_int_grid(key, index, x, y, integer(args, "value"))
            }
            "paint_tile" | "erase_tile" => {
                let (x, y) = coordinates();
                in_bounds(layer, ["__cWid", "__cHei"], x, y)?;
                let erase = name == "erase_tile";
                let (tile, src, flips) = if erase {
                    (0, [0, 0], 0)
                } else {
                    tile_values(args)
                };
                self.project
                    .paint_tile(key, index, x, y, tile, src, flips, erase)
            }
            "add_entity" => {
                let (x, y) = coordinates();
                let level = self.project.level(key).expect("resolved level");
                in_bounds(level, ["pxWid", "pxHei"], x, y)?;
                let position = layer_local_position(layer, x, y)?;
                let changed = self.project.place_entity(
                    key,
                    index,
                    args["def_uid"].as_i64().unwrap(),
                    [x, y],
                    position,
                );
                if matches!(changed, Ok(true)) {
                    let updated = self.project.layer(key, index).expect("resolved layer");
                    placed_iid = items(&updated["entityInstances"])
                        .last()
                        .map(|e| e["iid"].clone());
                }
                changed
            }
            "move_entity" | "remove_entity" => {
                let iid = args["entity_iid"].as_str().unwrap();
                let (entity_index, entity) = items(&layer["entityInstances"])
                    .enumerate()
                    .find(|(_, entity)| entity["iid"] == iid)
                    .ok_or_else(|| format!("Entity IID not found: {iid}"))?;
                if name == "remove_entity" {
                    self.project.remove_entity(key, index, entity_index)
                } else {
                    let (x, y) = coordinates();
                    let level = self.project.level(key).expect("resolved level");
                    in_bounds(level, ["pxWid", "pxHei"], x, y)?;
                    let [local_x, local_y] = layer_local_position(layer, x, y)?;
                    if entity["px"] == json!([local_x, local_y]) {
                        Ok(false)
                    } else {
                        self.project
                            .move_entity(key, index, entity_index, local_x, local_y)
                            .map(|()| true)
                    }
                }
            }
            _ => unreachable!("registered editing tool"),
        }
        .map_err(|error| error.to_string())?;
        self.dirty |= changed;
        let mut result = json!({"changed":changed,"dirty":self.dirty});
        if let Some(iid) = placed_iid {
            result["entity_iid"] = iid;
        }
        Ok(result)
    }

    fn edit_cells(
        &mut self,
        name: &str,
        key: LevelKey,
        layer_index: usize,
        args: &Arguments,
    ) -> ToolResult {
        let layer = self
            .project
            .layer(key, layer_index)
            .expect("resolved layer");
        let cells = items(&args["cells"]).enumerate().map(|(index, cell)| {
            let cell = cell.as_object().expect("validated cell");
            let (x, y) = (integer(cell, "x"), integer(cell, "y"));
            in_bounds(layer, ["__cWid", "__cHei"], x, y)
                .map_err(|error| format!("cells[{index}]: {error}"))?;
            Ok(([x, y], cell))
        });
        let count = match name {
            "paint_int_grid_cells" => {
                let edits = cells
                    .map(|cell| cell.map(|(position, args)| (position, integer(args, "value"))))
                    .collect::<Result<Vec<_>, String>>()?;
                self.project.paint_int_grid_cells(key, layer_index, &edits)
            }
            "paint_tile_cells" => {
                let edits = cells
                    .map(|cell| {
                        let (cell, args) = cell?;
                        let (id, src, flips) = tile_values(args);
                        Ok((
                            TilePaint {
                                cell,
                                id,
                                src,
                                stack: false,
                            },
                            flips,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                self.project
                    .paint_tile_cells(key, layer_index, &edits)
                    .map(|changed| {
                        changed
                            .into_iter()
                            .map(|index| edits[index].0.cell)
                            .collect::<HashSet<_>>()
                            .len()
                    })
            }
            "erase_tile_cells" => {
                let edits = cells
                    .map(|cell| cell.map(|(position, _)| position))
                    .collect::<Result<Vec<_>, String>>()?;
                self.project
                    .erase_tile_cells_with_deferred(key, layer_index, &edits, None)
                    .map(|changed| changed.len())
            }
            _ => unreachable!("registered batch editing tool"),
        }
        .map_err(|error| error.to_string())?;
        let changed = count > 0;
        self.dirty |= changed;
        Ok(json!({"changed":changed,"changed_cells":count,"dirty":self.dirty}))
    }

    fn save(&mut self) -> ToolResult {
        if !self.dirty {
            return Ok(json!({"saved":false,"dirty":false}));
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or("Reload the project before saving again.")?;
        self.check_external_save_targets()?;
        let reload = self
            .project
            .save_and_reload_if_unchanged(&self.path, snapshot)
            .map_err(|error| error.to_string())?;
        self.dirty = false;
        match reload {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.level_keys = OnceCell::new();
                Ok(json!({"saved":true,"dirty":false}))
            }
            Err(error) => {
                self.snapshot = None;
                Ok(json!({
                    "saved":true,"dirty":false,"reload_required":true,"message":error.to_string()
                }))
            }
        }
    }

    fn check_external_save_targets(&self) -> Result<(), String> {
        if self.project.json["externalLevels"] != true {
            return Ok(());
        }
        let base = self.path.parent().expect("absolute project path");
        let sources: HashSet<_> = self
            .project
            .external_levels
            .keys()
            .filter_map(|path| base.join(path).canonicalize().ok())
            .collect();
        // The core may rename legacy external level paths on save. An existing
        // file at a derived destination must belong to the loaded project.
        for (index, key) in self.project.iter_level_keys().enumerate() {
            let level = self.project.level(key).expect("loaded level");
            let relative = self
                .project
                .external_level_rel_path(&self.path, index, level)
                .map_err(|_| "Unsafe external level identifier.")?;
            let target = base.join(relative);
            match fs::symlink_metadata(&target) {
                Ok(_)
                    if target
                        .canonicalize()
                        .is_ok_and(|path| sources.contains(&path)) => {}
                Ok(_) => {
                    return Err(format!(
                        "External level destination is already in use: {}",
                        target.display()
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Cannot inspect external level destination {}: {error}",
                        target.display()
                    ));
                }
            }
        }
        Ok(())
    }
}

fn items(value: &Value) -> impl DoubleEndedIterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn integer(args: &Arguments, name: &str) -> i32 {
    args[name].as_i64().expect("validated i32") as i32
}

fn tile_values(args: &Arguments) -> (i32, [i32; 2], i32) {
    (
        integer(args, "tile_id"),
        [integer(args, "src_x"), integer(args, "src_y")],
        args.get("flips").and_then(Value::as_i64).unwrap_or(0) as i32,
    )
}

fn layer_local_position(layer: &Value, x: i32, y: i32) -> Result<[i32; 2], &'static str> {
    let local = |coordinate: i32, field: &str| {
        let offset = layer[field].as_i64().unwrap_or(0);
        i64::from(coordinate)
            .checked_sub(offset)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or("Layer-local entity coordinates exceed the supported integer range.")
    };
    Ok([local(x, "__pxTotalOffsetX")?, local(y, "__pxTotalOffsetY")?])
}

fn in_bounds(value: &Value, dimensions: [&str; 2], x: i32, y: i32) -> Result<(), String> {
    if [x, y].into_iter().zip(dimensions).all(|(position, field)| {
        position >= 0
            && value[field]
                .as_i64()
                .is_some_and(|limit| i64::from(position) < limit)
    }) {
        Ok(())
    } else {
        Err(format!(
            "Coordinates ({x}, {y}) are outside the requested bounds."
        ))
    }
}
