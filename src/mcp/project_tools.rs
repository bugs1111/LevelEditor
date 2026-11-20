use ldtk_core::{FilesystemSnapshot, LevelKey, Project};
use serde_json::{Map, Value, json};
use std::{collections::HashSet, fs, path::PathBuf};

type Arguments = Map<String, Value>;
type ToolResult = Result<Value, String>;

#[derive(Clone, Copy)]
enum Kind {
    Iid,
    Int32,
    Int64,
    Bool,
}

#[derive(Clone, Copy)]
struct Argument(&'static str, Kind, bool);

impl Argument {
    fn schema(self) -> Value {
        match self.1 {
            Kind::Iid => json!({"type":"string","minLength":1}),
            Kind::Int32 => json!({"type":"integer","minimum":i32::MIN,"maximum":i32::MAX}),
            Kind::Int64 => json!({"type":"integer","minimum":i64::MIN,"maximum":i64::MAX}),
            Kind::Bool => json!({"type":"boolean"}),
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self.1 {
            Kind::Iid => value.as_str().is_some_and(|iid| !iid.is_empty()),
            Kind::Int32 => value.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
            Kind::Int64 => value.is_i64(),
            Kind::Bool => value.is_boolean(),
        }
    }
}

const LEVEL: Argument = Argument("level_iid", Kind::Iid, true);
const LAYER: Argument = Argument("layer_iid", Kind::Iid, true);
const ENTITY: Argument = Argument("entity_iid", Kind::Iid, true);
const X: Argument = Argument("x", Kind::Int32, true);
const Y: Argument = Argument("y", Kind::Int32, true);

struct Tool {
    name: &'static str,
    description: &'static str,
    arguments: &'static [Argument],
    read_only: bool,
}

const fn read(
    name: &'static str,
    description: &'static str,
    arguments: &'static [Argument],
) -> Tool {
    Tool {
        name,
        description,
        arguments,
        read_only: true,
    }
}

const fn edit(
    name: &'static str,
    description: &'static str,
    arguments: &'static [Argument],
) -> Tool {
    Tool {
        name,
        description,
        arguments,
        read_only: false,
    }
}

const TOOLS: &[Tool] = &[
    read(
        "project_info",
        "Get the bound project's path, LDtk version and unsaved status.",
        &[],
    ),
    read(
        "get_definitions",
        "Read LDtk definitions, including layer palettes, entity definitions and tilesets.",
        &[],
    ),
    read(
        "list_worlds",
        "List worlds and their stable IIDs, including the project's top-level world.",
        &[],
    ),
    read(
        "list_levels",
        "List level summaries and stable IIDs, optionally filtered by a world IID.",
        &[Argument("world_iid", Kind::Iid, false)],
    ),
    read(
        "get_level",
        "Read level metadata and layer summaries by level IID.",
        &[LEVEL],
    ),
    read(
        "get_layer",
        "Read an LDtk layer instance, including its IntGrid, tiles or entities.",
        &[LEVEL, LAYER],
    ),
    edit(
        "paint_int_grid",
        "Set one IntGrid cell at zero-based layer grid coordinates x and y. Use a defined palette value or 0 to clear.",
        &[LEVEL, LAYER, X, Y, Argument("value", Kind::Int32, true)],
    ),
    edit(
        "paint_tile",
        "Replace tiles at one Tiles layer cell (grid x and y). Supply the atlas tile ID and its source pixel origin (src_x, src_y). Flips range from 0 to 3.",
        &[
            LEVEL,
            LAYER,
            X,
            Y,
            Argument("tile_id", Kind::Int32, true),
            Argument("src_x", Kind::Int32, true),
            Argument("src_y", Kind::Int32, true),
            Argument("flips", Kind::Int32, false),
        ],
    ),
    edit(
        "erase_tile",
        "Erase the tiles at one cell (grid x and y) on a Tiles layer.",
        &[LEVEL, LAYER, X, Y],
    ),
    edit(
        "add_entity",
        "Place an entity of def_uid at level-local pixel coordinates x and y. Uses the definition's layer filters and count limit behavior.",
        &[LEVEL, LAYER, Argument("def_uid", Kind::Int64, true), X, Y],
    ),
    edit(
        "move_entity",
        "Move an existing entity by IID to level-local pixel coordinates x and y.",
        &[LEVEL, LAYER, ENTITY, X, Y],
    ),
    edit(
        "remove_entity",
        "Remove an entity by IID and refresh its references.",
        &[LEVEL, LAYER, ENTITY],
    ),
    edit(
        "save_project",
        "Save in-memory changes to the bound project and its external levels after checking for disk changes.",
        &[],
    ),
    edit(
        "reload_project",
        "Reload the bound project from disk. Use discard_changes=true to explicitly discard unsaved changes.",
        &[Argument("discard_changes", Kind::Bool, false)],
    ),
];

pub(super) fn list() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|tool| {
            let properties: Map<String, Value> = tool
                .arguments
                .iter()
                .map(|arg| (arg.0.into(), arg.schema()))
                .collect();
            let required: Vec<_> = tool
                .arguments
                .iter()
                .filter(|arg| arg.2)
                .map(|arg| arg.0)
                .collect();
            json!({
                "name":tool.name,
                "description":tool.description,
                "inputSchema":{
                    "type":"object",
                    "properties":properties,
                    "required":required,
                    "additionalProperties":false
                },
                "annotations":{"readOnlyHint":tool.read_only,"openWorldHint":false}
            })
        })
        .collect()
}

pub(super) struct Session {
    path: PathBuf,
    project: Project,
    snapshot: Option<FilesystemSnapshot>,
    dirty: bool,
}

impl Session {
    pub(super) fn open(path: PathBuf) -> ldtk_core::Result<Self> {
        // Find the external files before taking the snapshot. Load the editable
        // copy against that snapshot so an edit during loading cannot go unnoticed.
        let discovered = Project::load(&path)?;
        let snapshot = discovered.filesystem_snapshot()?;
        let project = Project::load(&path)?;
        if discovered.json != project.json
            || discovered.external_levels != project.external_levels
            || !snapshot.changed_files()?.is_empty()
        {
            return Err(ldtk_core::Error::Invalid(
                "Project files changed during reload. Try reloading again.".into(),
            ));
        }
        Ok(Self {
            path,
            project,
            snapshot: Some(snapshot),
            dirty: false,
        })
    }

    pub(super) fn call(&mut self, name: &str, args: &Arguments) -> Option<ToolResult> {
        let tool = TOOLS.iter().find(|tool| tool.name == name)?;
        for key in args.keys() {
            if !tool.arguments.iter().any(|arg| arg.0 == key) {
                return Some(Err(format!("Unexpected argument: {key}")));
            }
        }
        for &arg in tool.arguments {
            match args.get(arg.0) {
                Some(value) if !arg.accepts(value) => {
                    return Some(Err(format!("Invalid argument: {}", arg.0)));
                }
                None if arg.2 => return Some(Err(format!("Missing argument: {}", arg.0))),
                _ => {}
            }
        }
        Some(self.execute(name, args))
    }

    fn execute(&mut self, name: &str, args: &Arguments) -> ToolResult {
        match name {
            "project_info" => Ok(self.info()),
            "get_definitions" => Ok(self.project.json["defs"]
                .as_object()
                .cloned()
                .map_or_else(|| json!({}), Value::Object)),
            "list_worlds" => Ok(json!({"worlds":self.worlds()})),
            "list_levels" => self.list_levels(args.get("world_iid").and_then(Value::as_str)),
            "get_level" => self.get_level(args[LEVEL.0].as_str().unwrap()),
            "get_layer" => {
                let (key, layer) = self.layer_key(args)?;
                Ok(json!({"layer":self.project.layer(key, layer)}))
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
        }
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

    fn world_iid(&self, key: LevelKey) -> Option<&str> {
        match key.world {
            Some(index) => self.project.json["worlds"][index]["iid"].as_str(),
            None => Some(self.root_world_iid()),
        }
    }

    fn worlds(&self) -> Vec<Value> {
        let root = &self.project.json;
        let mut worlds: Vec<_> = items(&root["worlds"])
            .map(|world| {
                json!({
                    "iid":world["iid"],
                    "identifier":world["identifier"],
                    "layout":world["worldLayout"]
                })
            })
            .collect();
        if worlds.is_empty() || items(&root["levels"]).next().is_some() {
            worlds.insert(
                0,
                json!({
                    "iid":self.root_world_iid(),
                    "identifier":"World",
                    "layout":root["worldLayout"]
                }),
            );
        }
        worlds
    }

    fn list_levels(&self, world_iid: Option<&str>) -> ToolResult {
        if let Some(iid) = world_iid
            && !self.worlds().iter().any(|world| world["iid"] == iid)
        {
            return Err(format!("World IID not found: {iid}"));
        }
        let levels: Vec<_> = self
            .project
            .level_keys()
            .into_iter()
            .filter(|&key| world_iid.is_none_or(|iid| self.world_iid(key) == Some(iid)))
            .filter_map(|key| {
                self.project
                    .level(key)
                    .map(|level| self.level_summary(key, level))
            })
            .collect();
        Ok(json!({"levels":levels}))
    }

    fn level_summary(&self, key: LevelKey, level: &Value) -> Value {
        json!({
            "iid":level["iid"],
            "identifier":level["identifier"],
            "world_iid":self.world_iid(key),
            "world_x":level["worldX"],
            "world_y":level["worldY"],
            "px_width":level["pxWid"],
            "px_height":level["pxHei"]
        })
    }

    fn get_level(&self, iid: &str) -> ToolResult {
        let (key, level) = self.level(iid)?;
        let mut summary = self.level_summary(key, level);
        let layers: Vec<_> = items(&level["layerInstances"])
            .map(|layer| {
                json!({
                    "iid":layer["iid"],
                    "identifier":layer["__identifier"],
                    "type":layer["__type"],
                    "definition_uid":layer["layerDefUid"],
                    "columns":layer["__cWid"],
                    "rows":layer["__cHei"],
                    "grid_size":layer["__gridSize"],
                    "tileset_uid":layer["__tilesetDefUid"]
                })
            })
            .collect();
        summary["layers"] = json!(layers);
        summary["field_instances"] = level["fieldInstances"].clone();
        Ok(summary)
    }

    fn level(&self, iid: &str) -> Result<(LevelKey, &Value), String> {
        self.project
            .level_keys()
            .into_iter()
            .filter_map(|key| self.project.level(key).map(|level| (key, level)))
            .find(|(_, level)| level["iid"] == iid)
            .ok_or_else(|| format!("Level IID not found: {iid}"))
    }

    fn layer_key(&self, args: &Arguments) -> Result<(LevelKey, usize), String> {
        let (key, level) = self.level(args[LEVEL.0].as_str().unwrap())?;
        let iid = args[LAYER.0].as_str().unwrap();
        let index = items(&level["layerInstances"])
            .position(|layer| layer["iid"] == iid)
            .ok_or_else(|| format!("Layer IID not found: {iid}"))?;
        Ok((key, index))
    }

    fn edit(&mut self, name: &str, args: &Arguments) -> ToolResult {
        let (key, index) = self.layer_key(args)?;
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
                let (tile, src) = if erase {
                    (0, [0, 0])
                } else {
                    (
                        integer(args, "tile_id"),
                        [integer(args, "src_x"), integer(args, "src_y")],
                    )
                };
                let flips = args.get("flips").and_then(Value::as_i64).unwrap_or(0) as i32;
                self.project
                    .paint_tile(key, index, x, y, tile, src, flips, erase)
            }
            "add_entity" => {
                let (x, y) = coordinates();
                let level = self.project.level(key).expect("resolved level");
                in_bounds(level, ["pxWid", "pxHei"], x, y)?;
                let changed =
                    self.project
                        .add_entity(key, index, args["def_uid"].as_i64().unwrap(), x, y);
                if matches!(changed, Ok(true)) {
                    let updated = self.project.layer(key, index).expect("resolved layer");
                    placed_iid = items(&updated["entityInstances"])
                        .last()
                        .map(|e| e["iid"].clone());
                }
                changed
            }
            "move_entity" | "remove_entity" => {
                let iid = args[ENTITY.0].as_str().unwrap();
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
                    if entity["px"] == json!([x, y]) {
                        Ok(false)
                    } else {
                        self.project
                            .move_entity(key, index, entity_index, x, y)
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

    fn save(&mut self) -> ToolResult {
        if !self.dirty {
            return Ok(json!({"saved":false,"dirty":false}));
        }
        let snapshot = self
            .snapshot
            .as_ref()
            .ok_or("Reload the project before saving again.")?;
        self.check_external_save_targets()?;
        self.project
            .save_if_unchanged(&self.path, snapshot)
            .map_err(|error| error.to_string())?;
        self.dirty = false;
        match Self::open(self.path.clone()) {
            Ok(session) => {
                *self = session;
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
        let directory = self
            .project
            .external_files_directory()
            .expect("loaded project");
        let sources: HashSet<_> = self
            .project
            .external_levels
            .keys()
            .filter_map(|path| base.join(path).canonicalize().ok())
            .collect();
        let indexed =
            items(&self.project.json["flags"]).any(|flag| flag == "PrependIndexToLevelFileNames");
        // The core may rename legacy external level paths on save. An existing
        // file at a derived destination must belong to the loaded project.
        for (index, key) in self.project.level_keys().into_iter().enumerate() {
            let level = self.project.level(key).expect("loaded level");
            let identifier = level["identifier"].as_str().unwrap_or("Level");
            if identifier.contains(['/', '\\']) || matches!(identifier, "." | "..") {
                return Err("Unsafe external level identifier.".into());
            }
            let prefix = if indexed {
                format!("{index:04}-")
            } else {
                String::new()
            };
            let target = directory.join(format!("./{prefix}{identifier}.ldtkl"));
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
