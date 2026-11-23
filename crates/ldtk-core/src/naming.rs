//! Original identifier conventions and automatic world level names.
use crate::{
    Project, Result,
    workflow::{invalid, visit_fields_mut},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(crate) fn cleanup(input: &str, style: &str) -> String {
    let mut clean = String::new();
    for ch in input.trim().chars() {
        let ch = if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        };
        if ch != '_' || !clean.ends_with('_') {
            clean.push(ch);
        }
    }
    let leading = clean.bytes().take_while(u8::is_ascii_digit).count();
    if leading > 0 {
        clean.replace_range(..leading, "_");
    }
    while clean.contains("__") {
        clean = clean.replace("__", "_");
    }
    match style {
        "Uppercase" => clean.make_ascii_uppercase(),
        "Lowercase" => clean.make_ascii_lowercase(),
        "Capitalize" => {
            if let Some(index) = clean.bytes().position(|c| c != b'_')
                && clean.as_bytes()[index].is_ascii_lowercase()
            {
                clean[index..index + 1].make_ascii_uppercase();
            }
        }
        _ => {}
    }
    clean
}
pub(crate) fn unique_name(input: &str, style: &str, occupied: &BTreeSet<String>) -> String {
    available_name(input, style, |name| occupied.contains(name), None)
}

fn available_name(
    input: &str,
    style: &str,
    occupied: impl Fn(&str) -> bool,
    next_suffix: Option<&mut HashMap<String, u64>>,
) -> String {
    let mut base = cleanup(input, style);
    if base.is_empty() || base == "_" {
        base = cleanup("Unnamed", style);
    }
    if !occupied(&base) {
        return base;
    }
    let prefix = base.trim_end_matches(|c: char| c.is_ascii_digit());
    let mut suffix = next_suffix
        .as_deref()
        .and_then(|cache| cache.get(&base))
        .copied()
        .unwrap_or_else(|| base[prefix.len()..].parse().unwrap_or(2));
    loop {
        let candidate = format!("{prefix}{suffix}");
        suffix = suffix.checked_add(1).unwrap_or(2);
        if !occupied(&candidate) {
            if let Some(cache) = next_suffix {
                cache.insert(base, suffix);
            }
            return candidate;
        }
    }
}
fn replace_string(value: &mut Value, replacements: &BTreeMap<String, String>) {
    if let Some(next) = value.as_str().and_then(|s| replacements.get(s)) {
        *value = json!(next);
    }
}
fn rename_definition(def: &mut Value, style: &str) {
    if let Some(name) = def["identifier"].as_str() {
        def["identifier"] = json!(cleanup(name, style));
    }
}
struct EnumNames {
    uid: i64,
    before: String,
    after: String,
    values: BTreeMap<String, String>,
}
fn update_field_def(def: &mut Value, enums: &[EnumNames], fields: &mut BTreeMap<i64, usize>) {
    let Some(kind) = def["__type"].as_str() else {
        return;
    };
    let array = kind.starts_with("Array<");
    let base = kind
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(kind);
    let Some(index) = enums
        .iter()
        .position(|e| base == format!("LocalEnum.{}", e.before))
    else {
        return;
    };
    let names = &enums[index];
    let kind = format!("LocalEnum.{}", names.after);
    def["__type"] = json!(if array {
        format!("Array<{kind}>")
    } else {
        kind
    });
    if let Some(params) = def["defaultOverride"]["params"].as_array_mut() {
        for value in params {
            replace_string(value, &names.values);
        }
    }
    if let Some(uid) = def["uid"].as_i64() {
        fields.insert(uid, index);
    }
}

impl Project {
    /// Apply the original project convention. Custom field names, IntGrid value
    /// names, arbitrary tags and externally imported enums keep their case.
    pub fn apply_identifier_style(&mut self, style: &str) -> Result<()> {
        if !["Capitalize", "Uppercase", "Lowercase", "Free"].contains(&style) {
            return Err(invalid("unknown identifier style"));
        }
        if self.json["identifierStyle"] == style {
            return Ok(());
        }
        self.edit_document(|p| {
            let enums = p.json["defs"]["enums"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    let before = e["identifier"].as_str().unwrap_or("").to_owned();
                    EnumNames {
                        uid: e["uid"].as_i64().unwrap_or(-1),
                        after: cleanup(&before, style),
                        before,
                        values: e["values"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|v| v["id"].as_str())
                            .map(|s| (s.to_owned(), cleanup(s, style)))
                            .collect(),
                    }
                })
                .collect::<Vec<_>>();
            p.json["identifierStyle"] = json!(style);
            for kind in ["enums", "layers", "entities", "tilesets"] {
                for def in p.json["defs"][kind].as_array_mut().into_iter().flatten() {
                    rename_definition(def, style);
                    if kind == "enums" {
                        for v in def["values"].as_array_mut().into_iter().flatten() {
                            if let Some(id) = v["id"].as_str() {
                                v["id"] = json!(cleanup(id, style));
                            }
                        }
                    }
                }
            }
            let mut fields = BTreeMap::new();
            for def in p.json["defs"]["levelFields"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                update_field_def(def, &enums, &mut fields);
            }
            for entity in p.json["defs"]["entities"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                for def in entity["fieldDefs"].as_array_mut().into_iter().flatten() {
                    update_field_def(def, &enums, &mut fields);
                }
            }
            for tileset in p.json["defs"]["tilesets"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                if let Some(names) = enums.iter().find(|e| tileset["tagsSourceEnumUid"] == e.uid) {
                    for tag in tileset["enumTags"].as_array_mut().into_iter().flatten() {
                        replace_string(&mut tag["enumValueId"], &names.values);
                    }
                }
            }
            for layer in p.json["defs"]["layers"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                if let Some(names) = layer["biomeFieldUid"]
                    .as_i64()
                    .and_then(|uid| fields.get(&uid))
                    .map(|i| &enums[*i])
                {
                    for group in layer["autoRuleGroups"].as_array_mut().into_iter().flatten() {
                        for value in group["requiredBiomeValues"]
                            .as_array_mut()
                            .into_iter()
                            .flatten()
                        {
                            replace_string(value, &names.values);
                        }
                    }
                }
            }
            for world in p.json["worlds"].as_array_mut().into_iter().flatten() {
                rename_definition(world, style);
            }
            for key in p.level_keys() {
                let level = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("all external levels must be loaded"))?;
                rename_definition(level, style);
                visit_fields_mut(level, |field| {
                    let Some(names) = field["defUid"]
                        .as_i64()
                        .and_then(|uid| fields.get(&uid))
                        .map(|i| &enums[*i])
                    else {
                        return;
                    };
                    if let Some(array) = field["__value"].as_array_mut() {
                        for value in array {
                            replace_string(value, &names.values);
                        }
                    } else {
                        replace_string(&mut field["__value"], &names.values);
                    }
                    for value in field["realEditorValues"]
                        .as_array_mut()
                        .into_iter()
                        .flatten()
                    {
                        for param in value["params"].as_array_mut().into_iter().flatten() {
                            replace_string(param, &names.values);
                        }
                    }
                    if let Some(params) = field["realEditorValue"]["params"].as_array_mut() {
                        for value in params {
                            replace_string(value, &names.values);
                        }
                    }
                });
            }
            p.refresh_derived_data()?;
            p.refresh_level_names()
        })
    }

    /// Refresh automatic names in world/level order, reserving manual names
    /// throughout the project. Does not recurse into derived-data refresh.
    pub fn refresh_level_names(&mut self) -> Result<()> {
        let mut occupied: HashMap<String, usize> = HashMap::new();
        for key in self.iter_level_keys() {
            let level = self
                .level(key)
                .ok_or_else(|| invalid("all external levels must be loaded"))?;
            if let Some(name) = level["identifier"].as_str() {
                *occupied.entry(name.to_owned()).or_default() += 1;
            }
        }
        let style = self.json["identifierStyle"]
            .as_str()
            .unwrap_or("Capitalize")
            .to_owned();
        let pattern = self.json["levelNamePattern"]
            .as_str()
            .unwrap_or("Level_%idx")
            .to_owned();
        for world in self.world_indices() {
            let mut origin = [i64::MAX; 2];
            let mut automatic = Vec::new();
            for key in self.level_keys_in_world(world) {
                let level = self.level(key).expect("validated level");
                for (axis, field) in ["worldX", "worldY"].iter().enumerate() {
                    origin[axis] = origin[axis].min(level[field].as_i64().unwrap_or(0));
                }
                let name = level["identifier"].as_str();
                if level["useAutoIdentifier"] == true {
                    // Only this world's old automatic names become available.
                    if let Some(name) = name {
                        *occupied.get_mut(name).expect("counted level name") -= 1;
                    }
                    automatic.push(key);
                } else {
                    self.set_level_name(key, name.unwrap_or("").to_owned());
                }
            }
            if automatic.is_empty() {
                continue;
            }
            let container = world
                .and_then(|i| self.json["worlds"].get(i))
                .unwrap_or(&self.json);
            let world_name = container["identifier"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| cleanup("World", &style));
            let layout = container["worldLayout"]
                .as_str()
                .unwrap_or("Free")
                .to_owned();
            let grid = [
                container["worldGridWidth"].as_i64().unwrap_or(256).max(1),
                container["worldGridHeight"].as_i64().unwrap_or(256).max(1),
            ];
            // Reservations only grow until we begin the next world.
            let mut next_suffix = HashMap::new();
            for key in automatic {
                let index = key.level;
                let level = self.level(key).expect("validated level");
                let (x, y, depth) = (
                    level["worldX"].as_i64().unwrap_or(0),
                    level["worldY"].as_i64().unwrap_or(0),
                    level["worldDepth"].as_i64().unwrap_or(0),
                );
                let grid_value = |value: i64, axis: usize| {
                    if layout == "GridVania" {
                        ((value - origin[axis]) / grid[axis]).to_string()
                    } else {
                        String::new()
                    }
                };
                let position = |value: i64| {
                    if layout == "Free" || layout == "GridVania" {
                        value.to_string()
                    } else {
                        String::new()
                    }
                };
                let mut name = pattern.clone();
                for (token, value) in [
                    ("%depth", depth.to_string()),
                    ("%world", world_name.clone()),
                    ("%idx1", (index + 1).to_string()),
                    ("%idx", index.to_string()),
                    ("%gx", grid_value(x, 0)),
                    ("%gy", grid_value(y, 1)),
                    ("%x", position(x)),
                    ("%y", position(y)),
                    ("%d", depth.to_string()),
                ] {
                    name = name.replace(token, &value);
                }
                let name = available_name(
                    &name,
                    &style,
                    |candidate| occupied.get(candidate).copied().unwrap_or(0) > 0,
                    Some(&mut next_suffix),
                );
                *occupied.entry(name.clone()).or_default() += 1;
                self.set_level_name(key, name);
            }
        }
        Ok(())
    }
}
