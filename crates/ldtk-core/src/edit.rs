use crate::{Error, LevelKey, Project, Result};
use serde_json::{Value, json};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn integer(value: &Value, name: &str) -> Result<i64> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid(&format!("missing or invalid {name}")))
}
fn dimensions(layer: &Value) -> Result<(i32, i32, i32)> {
    let width =
        i32::try_from(integer(layer, "__cWid")?).map_err(|_| invalid("invalid layer width"))?;
    let height =
        i32::try_from(integer(layer, "__cHei")?).map_err(|_| invalid("invalid layer height"))?;
    let grid =
        i32::try_from(integer(layer, "__gridSize")?).map_err(|_| invalid("invalid grid size"))?;
    if width <= 0
        || height <= 0
        || grid <= 0
        || width.checked_mul(height).is_none()
        || width.checked_mul(grid).is_none()
        || height.checked_mul(grid).is_none()
    {
        return Err(invalid(
            "layer dimensions must be positive and fit within i32",
        ));
    }
    Ok((width, height, grid))
}
fn inside(x: i32, y: i32, width: i32, height: i32) -> bool {
    x >= 0 && y >= 0 && x < width && y < height
}

impl Project {
    pub fn remove_entity(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        index: usize,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Entities" {
            return Err(invalid("entity removal requires an Entities layer"));
        }
        let entities = layer
            .get("entityInstances")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("entityInstances must be an array"))?;
        let Some(entity) = entities.get(index) else {
            return Ok(false);
        };
        let uid = integer(entity, "defUid")?;
        self.ensure_toc_editable(uid)?;
        let iid = entity
            .get("iid")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("entity IID is missing"))?;
        self.ensure_entity_unreferenced(iid)?;
        self.layer_mut(key, layer_index).expect("validated layer")["entityInstances"]
            .as_array_mut()
            .expect("validated entity list")
            .remove(index);
        Ok(true)
    }

    fn ensure_toc_editable(&self, uid: i64) -> Result<()> {
        let definition = self
            .definition("entities", uid)
            .ok_or_else(|| invalid("entity definition not found"))?;
        if definition.get("exportToToc").and_then(Value::as_bool) == Some(true) {
            return Err(Error::Unsupported("editing entities exported to the table of contents requires derived toc regeneration".into()));
        }
        Ok(())
    }

    fn ensure_entity_unreferenced(&self, iid: &str) -> Result<()> {
        fn contains_iid(value: &Value, iid: &str) -> bool {
            match value {
                Value::String(text) => text == iid,
                Value::Array(values) => values.iter().any(|value| contains_iid(value, iid)),
                Value::Object(values) => values.values().any(|value| contains_iid(value, iid)),
                _ => false,
            }
        }
        fn check_fields(fields: Option<&Value>, iid: &str) -> Result<()> {
            let Some(fields) = fields else {
                return Ok(());
            };
            let fields = fields.as_array().ok_or_else(|| {
                invalid("fieldInstances must be an array before checking references")
            })?;
            for field in fields {
                if field
                    .get("__type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| !kind.contains("EntityRef"))
                {
                    continue;
                }
                if field
                    .get("__value")
                    .is_some_and(|value| contains_iid(value, iid))
                    || field
                        .get("realEditorValues")
                        .is_some_and(|value| contains_iid(value, iid))
                    || field
                        .get("realEditorValue")
                        .is_some_and(|value| contains_iid(value, iid))
                {
                    return Err(Error::Unsupported("entity is referenced by a field; removing referenced entities requires a project-wide reference transaction".into()));
                }
            }
            Ok(())
        }
        for key in self.level_keys() {
            let level = self.level(key).ok_or_else(|| {
                invalid("all external levels must be loaded before checking entity references")
            })?;
            check_fields(level.get("fieldInstances"), iid)?;
            let layers = level
                .get("layerInstances")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    invalid("all level layers must be loaded before checking entity references")
                })?;
            for layer in layers {
                if let Some(entities) = layer.get("entityInstances") {
                    for entity in entities
                        .as_array()
                        .ok_or_else(|| invalid("entityInstances must be an array"))?
                    {
                        check_fields(entity.get("fieldInstances"), iid)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn paint_int_grid(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        value: i32,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "IntGrid" {
            return Err(invalid("IntGrid painting requires an IntGrid layer"));
        }
        let (width, height, _) = dimensions(layer)?;
        let cells = layer
            .get("intGridCsv")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("intGridCsv must be an array"))?;
        if cells.len() != (width as usize) * (height as usize) {
            return Err(invalid(
                "IntGrid cell count or values do not match the layer dimensions",
            ));
        }
        if value < 0 {
            return Err(invalid("IntGrid values must be nonnegative"));
        }
        if !inside(x, y, width, height) {
            return Ok(false);
        }
        let index = (y * width + x) as usize;
        if cells[index]
            .as_i64()
            .is_none_or(|n| i32::try_from(n).is_err() || n < 0)
        {
            return Err(invalid("invalid IntGrid cell value"));
        }
        if value != 0 {
            let definition = layer
                .get("layerDefUid")
                .and_then(Value::as_i64)
                .and_then(|uid| self.layer_def(uid))
                .ok_or_else(|| invalid("IntGrid layer definition not found"))?;
            let palette = definition
                .get("intGridValues")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("IntGrid definition has no value palette"))?;
            if !palette
                .iter()
                .any(|entry| entry.get("value").and_then(Value::as_i64) == Some(i64::from(value)))
            {
                return Err(invalid(
                    "IntGrid replacement value is not defined in this layer's palette",
                ));
            }
        }
        if cells[index] == value {
            return Ok(false);
        }
        let uid = layer.get("layerDefUid").and_then(Value::as_i64);
        if self
            .json
            .get("defs")
            .and_then(|defs| defs.get("layers"))
            .and_then(Value::as_array)
            .is_some_and(|defs| {
                defs.iter().any(|definition| {
                    (definition.get("uid").and_then(Value::as_i64) == uid
                        || definition
                            .get("autoSourceLayerDefUid")
                            .and_then(Value::as_i64)
                            == uid)
                        && active_auto_rules(definition)
                })
            })
        {
            return Err(Error::Unsupported("IntGrid editing with active auto rules requires deterministic autotile recomputation".into()));
        }
        self.layer_mut(key, layer_index).expect("validated layer")["intGridCsv"][index] =
            json!(value);
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_tile(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        tile_id: i32,
        src: [i32; 2],
        flips: i32,
        erase: bool,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Tiles" {
            return Err(invalid("manual tiles require a Tiles layer"));
        }
        let (width, height, grid) = dimensions(layer)?;
        if !inside(x, y, width, height) {
            return Ok(false);
        }
        if !erase && (tile_id < 0 || src.iter().any(|v| *v < 0) || !(0..=3).contains(&flips)) {
            return Err(invalid("invalid tile ID, source coordinates or flip flags"));
        }
        if !erase {
            let tileset_uid = layer
                .get("overrideTilesetUid")
                .and_then(Value::as_i64)
                .or_else(|| layer.get("__tilesetDefUid").and_then(Value::as_i64))
                .or_else(|| {
                    layer
                        .get("layerDefUid")
                        .and_then(Value::as_i64)
                        .and_then(|uid| self.layer_def(uid))
                        .and_then(|d| d.get("tilesetDefUid"))
                        .and_then(Value::as_i64)
                })
                .ok_or_else(|| invalid("Tiles layer has no assigned tileset"))?;
            let tileset = self
                .tileset(tileset_uid)
                .ok_or_else(|| invalid("tileset definition not found"))?;
            let tile_grid = integer(tileset, "tileGridSize")?;
            let atlas_width = integer(tileset, "pxWid")?;
            let atlas_height = integer(tileset, "pxHei")?;
            let padding = tileset.get("padding").and_then(Value::as_i64).unwrap_or(0);
            let spacing = tileset.get("spacing").and_then(Value::as_i64).unwrap_or(0);
            if tile_grid <= 0
                || padding < 0
                || spacing < 0
                || atlas_width < tile_grid
                || atlas_height < tile_grid
            {
                return Err(invalid("invalid atlas dimensions, padding or spacing"));
            }
            let stride = tile_grid
                .checked_add(spacing)
                .ok_or_else(|| invalid("atlas stride overflow"))?;
            let cells = |size: i64| {
                size.checked_sub(padding.checked_mul(2)?)
                    .and_then(|n| n.checked_add(spacing))
                    .map(|n| n / stride)
            };
            let cols = cells(atlas_width)
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid atlas width"))?;
            let rows = cells(atlas_height)
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid atlas height"))?;
            let count = cols
                .checked_mul(rows)
                .ok_or_else(|| invalid("atlas tile count overflow"))?;
            let id = i64::from(tile_id);
            if id >= count
                || i64::from(src[0]) != padding + (id % cols) * stride
                || i64::from(src[1]) != padding + (id / cols) * stride
            {
                return Err(invalid(
                    "tile ID and source coordinates do not match the assigned atlas",
                ));
            }
        }
        let tiles = layer
            .get("gridTiles")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("gridTiles must be an array"))?;
        if tiles.iter().any(|tile| {
            !tile.is_object()
                || tile
                    .get("px")
                    .and_then(Value::as_array)
                    .is_none_or(|p| p.len() != 2 || p.iter().any(|v| v.as_i64().is_none()))
        }) {
            return Err(invalid("tile pixel coordinates must be integer pairs"));
        }
        let px = json!([x * grid, y * grid]);
        let indices: Vec<usize> = tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| (tile["px"] == px).then_some(i))
            .collect();
        if erase && indices.is_empty() {
            return Ok(false);
        }
        if !erase && indices.len() == 1 {
            let tile = &tiles[indices[0]];
            if tile["t"] == tile_id && tile["src"] == json!(src) && tile["f"] == flips {
                return Ok(false);
            }
        }
        let mut replacement = indices
            .first()
            .map(|&i| tiles[i].clone())
            .unwrap_or_else(|| json!({"a":1,"d":[y*width+x]}));
        if !erase {
            replacement["px"] = px;
            replacement["src"] = json!(src);
            replacement["t"] = json!(tile_id);
            replacement["f"] = json!(flips);
        }
        let tiles = self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
            .as_array_mut()
            .expect("validated tiles");
        if let Some(&first) = indices.first() {
            for &index in indices.iter().rev() {
                tiles.remove(index);
            }
            if !erase {
                tiles.insert(first, replacement);
            }
        } else {
            tiles.push(replacement);
        }
        Ok(true)
    }

    pub fn add_entity(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        x: i32,
        y: i32,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Entities" {
            return Err(invalid("entity placement requires an Entities layer"));
        }
        let (_, _, grid) = dimensions(layer)?;
        layer
            .get("entityInstances")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("entityInstances must be an array"))?;
        let definition = self
            .definition("entities", def_uid)
            .ok_or_else(|| invalid("entity definition not found"))?
            .clone();
        let level = self.level(key).expect("validated level");
        let width = integer(level, "pxWid")?;
        let height = integer(level, "pxHei")?;
        if width <= 0 || height <= 0 {
            return Err(invalid("level dimensions must be positive"));
        }
        if x < 0 || y < 0 || i64::from(x) >= width || i64::from(y) >= height {
            return Ok(false);
        }
        self.ensure_toc_editable(def_uid)?;
        if let Some(layer_def) = layer
            .get("layerDefUid")
            .and_then(Value::as_i64)
            .and_then(|uid| self.layer_def(uid))
        {
            let tags = definition.get("tags").and_then(Value::as_array);
            let intersects = |name| {
                layer_def
                    .get(name)
                    .and_then(Value::as_array)
                    .is_some_and(|required| {
                        required
                            .iter()
                            .any(|t| tags.is_some_and(|tags| tags.contains(t)))
                    })
            };
            let requires_tags = layer_def
                .get("requiredTags")
                .and_then(Value::as_array)
                .is_some_and(|tags| !tags.is_empty());
            if (requires_tags && !intersects("requiredTags")) || intersects("excludedTags") {
                return Ok(false);
            }
        }
        let max = definition
            .get("maxCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let scope = definition
            .get("limitScope")
            .and_then(Value::as_str)
            .unwrap_or("PerLevel");
        let mut matches = Vec::new();
        if max > 0 {
            if !["PerLayer", "PerLevel", "PerWorld"].contains(&scope) {
                return Err(invalid("unknown entity count limit scope"));
            }
            for candidate_key in self.level_keys() {
                if candidate_key.world != key.world || (scope != "PerWorld" && candidate_key != key)
                {
                    continue;
                }
                let layers = self
                    .level(candidate_key)
                    .and_then(|l| l.get("layerInstances"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("level layers not loaded"))?;
                for (li, candidate) in layers.iter().enumerate() {
                    if scope == "PerLayer" && li != layer_index {
                        continue;
                    }
                    if let Some(entities) =
                        candidate.get("entityInstances").and_then(Value::as_array)
                    {
                        for (ei, entity) in entities.iter().enumerate() {
                            if entity["defUid"] == def_uid {
                                matches.push((candidate_key, li, ei));
                            }
                        }
                    }
                }
            }
        }
        let mut entity = build_entity(&definition, def_uid)?;
        let limit_reached = max > 0 && matches.len() as i64 >= max;
        let behavior = definition
            .get("limitBehavior")
            .and_then(Value::as_str)
            .unwrap_or("PreventAdding");
        if limit_reached {
            match behavior {
                "PreventAdding" => return Ok(false),
                "MoveLastOne" => {
                    let &(old_key, li, ei) = matches.last().expect("positive max reached");
                    if old_key != key {
                        return Err(Error::Unsupported(
                            "moving a limited entity between levels requires cross-level history"
                                .into(),
                        ));
                    }
                    entity = self.layer(old_key, li).expect("matched layer")["entityInstances"][ei]
                        .clone();
                    if li != layer_index {
                        let iid = entity
                            .get("iid")
                            .and_then(Value::as_str)
                            .ok_or_else(|| invalid("entity IID is missing"))?;
                        self.ensure_entity_unreferenced(iid)?;
                    }
                }
                "DiscardOldOnes" => {
                    let count = matches.len() - max as usize + 1;
                    for &(old_key, li, ei) in &matches[..count] {
                        let iid =
                            self.layer(old_key, li).expect("matched layer")["entityInstances"][ei]
                                .get("iid")
                                .and_then(Value::as_str)
                                .ok_or_else(|| invalid("entity IID is missing"))?;
                        self.ensure_entity_unreferenced(iid)?;
                    }
                    if matches[..count]
                        .iter()
                        .any(|(old_key, _, _)| *old_key != key)
                    {
                        return Err(Error::Unsupported("discarding limited entities in other levels requires cross-level history".into()));
                    }
                }
                _ => return Err(invalid("unknown entity limit behavior")),
            }
        }
        let pivot_x = definition
            .get("pivotX")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let pivot_y = definition
            .get("pivotY")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        entity["px"] = json!([x, y]);
        entity["__grid"] = json!([
            (x - i32::from(pivot_x == 1.0)) / grid,
            (y - i32::from(pivot_y == 1.0)) / grid
        ]);
        let world = key
            .world
            .and_then(|world| self.json.get("worlds")?.get(world))
            .unwrap_or(&self.json);
        let layout = world
            .get("worldLayout")
            .and_then(Value::as_str)
            .unwrap_or("Free");
        if layout == "Free" || layout == "GridVania" {
            entity["__worldX"] = json!(integer(level, "worldX").unwrap_or(0) + i64::from(x));
            entity["__worldY"] = json!(integer(level, "worldY").unwrap_or(0) + i64::from(y));
        }
        // Validate everything above before removing or inserting any existing instance.
        if limit_reached {
            let removals = if behavior == "MoveLastOne" {
                &matches[matches.len() - 1..]
            } else {
                &matches[..matches.len() - max as usize + 1]
            };
            for &(old_key, li, ei) in removals.iter().rev() {
                self.layer_mut(old_key, li).expect("matched layer")["entityInstances"]
                    .as_array_mut()
                    .expect("matched entity array")
                    .remove(ei);
            }
        }
        self.layer_mut(key, layer_index).expect("validated layer")["entityInstances"]
            .as_array_mut()
            .expect("validated entity array")
            .push(entity);
        Ok(true)
    }
}

fn build_entity(definition: &Value, uid: i64) -> Result<Value> {
    let identifier = definition
        .get("identifier")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("entity identifier missing"))?;
    let width = integer(definition, "width")?;
    let height = integer(definition, "height")?;
    if width <= 0 || height <= 0 {
        return Err(invalid("entity dimensions must be positive"));
    }
    let fields = definition
        .get("fieldDefs")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("entity fieldDefs must be an array"))?;
    let fields: Vec<Value> = fields.iter().map(default_field).collect::<Result<_>>()?;
    Ok(json!({
        "__identifier":identifier,"__grid":[0,0],"__pivot":[definition.get("pivotX").cloned().unwrap_or(json!(0)),definition.get("pivotY").cloned().unwrap_or(json!(0))],
        "__tags":definition.get("tags").cloned().unwrap_or(json!([])),"__tile":definition.get("tileRect").cloned().unwrap_or(Value::Null),
        "__smartColor":definition.get("color").cloned().unwrap_or(json!("#FFFFFF")),
        "iid":uuid::Uuid::new_v4().to_string(),"width":width,"height":height,"defUid":uid,"px":[0,0],"fieldInstances":fields
    }))
}

fn default_field(definition: &Value) -> Result<Value> {
    let uid = integer(definition, "uid")?;
    let identifier = definition
        .get("identifier")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("field identifier missing"))?;
    let kind = definition
        .get("__type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("field __type missing; migration required for this definition"))?;
    let is_array = definition
        .get("isArray")
        .and_then(Value::as_bool)
        .unwrap_or(kind.starts_with("Array<"));
    let base = kind
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(kind);
    let nullable = definition
        .get("canBeNull")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let override_value = definition
        .get("defaultOverride")
        .filter(|v| !v.is_null())
        .and_then(|v| v.get("params"))
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .cloned();
    let mut value = if let Some(value) = override_value {
        value
    } else if nullable {
        Value::Null
    } else {
        match base {
            "Int" | "Float" => json!(0),
            "Bool" => json!(false),
            "Color" => json!(0),
            "String" | "Multilines" => json!(""),
            "Point" | "EntityRef" | "Tile" | "FilePath" => Value::Null,
            _ if base.starts_with("LocalEnum.") || base.starts_with("ExternEnum.") => Value::Null,
            _ => return Err(Error::Unsupported(format!("new entity field type {kind}"))),
        }
    };
    if let Some(number) = value.as_f64().filter(|_| base == "Int" || base == "Float") {
        let number = definition
            .get("min")
            .and_then(Value::as_f64)
            .map_or(number, |min| number.max(min));
        let number = definition
            .get("max")
            .and_then(Value::as_f64)
            .map_or(number, |max| number.min(max));
        value = if base == "Int" {
            json!(number as i64)
        } else {
            json!(number)
        };
    }
    if base == "Color"
        && let Some(number) = value.as_i64()
    {
        value = json!(format!("#{:06X}", number & 0xFFFFFF));
    }
    if base == "Tile"
        && let Some(raw) = value.as_str()
    {
        let coords: Vec<i32> = raw
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| invalid("invalid tile field default"))?;
        if coords.len() != 4 {
            return Err(invalid("tile field default must contain x,y,w,h"));
        }
        value = json!({"tilesetUid":definition["tilesetUid"],"x":coords[0],"y":coords[1],"w":coords[2],"h":coords[3]});
    }
    let value = if is_array { json!([]) } else { value };
    Ok(
        json!({"__identifier":identifier,"__type":kind,"__value":value,"__tile":null,"defUid":uid,"realEditorValues":[]}),
    )
}

fn active_auto_rules(definition: &Value) -> bool {
    definition
        .get("autoRuleGroups")
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                group.get("active").and_then(Value::as_bool) != Some(false)
                    && group
                        .get("rules")
                        .and_then(Value::as_array)
                        .is_some_and(|rules| {
                            rules.iter().any(|rule| {
                                rule.get("active").and_then(Value::as_bool) != Some(false)
                            })
                        })
            })
        })
}
