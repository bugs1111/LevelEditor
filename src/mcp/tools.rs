use serde_json::{Map, Value, json};

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

pub(super) struct Tool {
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

impl Tool {
    pub(super) fn named(name: &str) -> Option<&'static Self> {
        TOOLS.iter().find(|tool| tool.name == name)
    }

    pub(super) fn validate(&self, args: &Map<String, Value>) -> Result<(), String> {
        for key in args.keys() {
            if !self.arguments.iter().any(|arg| arg.0 == key) {
                return Err(format!("Unexpected argument: {key}"));
            }
        }
        for &arg in self.arguments {
            match args.get(arg.0) {
                Some(value) if !arg.accepts(value) => {
                    return Err(format!("Invalid argument: {}", arg.0));
                }
                None if arg.2 => return Err(format!("Missing argument: {}", arg.0)),
                _ => {}
            }
        }
        Ok(())
    }
}
