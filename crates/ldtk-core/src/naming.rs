//! Original identifier conventions and automatic world level names.
use crate::{
    LevelKey, Project, Result,
    workflow::{invalid, visit_fields_mut},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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
    let mut base = cleanup(input, style);
    if base.is_empty() || base == "_" {
        base = cleanup("Unnamed", style);
    }
    if !occupied.contains(&base) {
        return base;
    }
    let split = base.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, mut suffix) = if split < base.len() {
        (
            base[..split].to_owned(),
            base[split..].parse::<u64>().unwrap_or(2),
        )
    } else {
        (base, 2)
    };
    loop {
        let candidate = format!("{prefix}{suffix}");
        if !occupied.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
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
        let keys = self.level_keys();
        for key in &keys {
            if self.level(*key).is_none() {
                return Err(invalid("all external levels must be loaded"));
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
        let mut worlds = Vec::new();
        for key in &keys {
            if !worlds.contains(&key.world) {
                worlds.push(key.world);
            }
        }
        for world in worlds {
            let current = keys
                .iter()
                .copied()
                .filter(|key| key.world == world)
                .collect::<Vec<_>>();
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
            let origin = ["worldX", "worldY"].map(|axis| {
                current
                    .iter()
                    .filter_map(|key| self.level(*key))
                    .map(|l| l[axis].as_i64().unwrap_or(0))
                    .min()
                    .unwrap_or(0)
            });
            let mut occupied = keys
                .iter()
                .filter_map(|key| self.level(*key).map(|l| (*key, l)))
                .filter(|(key, l)| key.world != world || l["useAutoIdentifier"] != true)
                .filter_map(|(_, l)| l["identifier"].as_str().map(str::to_owned))
                .collect::<BTreeSet<_>>();
            for (index, key) in current.into_iter().enumerate() {
                let level = self.level(key).expect("validated level");
                if level["useAutoIdentifier"] != true {
                    continue;
                }
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
                let name = unique_name(&name, &style, &occupied);
                occupied.insert(name.clone());
                self.set_level_name(key, name);
            }
        }
        // Inline summaries for external documents must expose the same identifier.
        for key in keys {
            let name = self.level(key).expect("validated level")["identifier"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            self.set_level_name(key, name);
        }
        Ok(())
    }
    fn set_level_name(&mut self, key: LevelKey, name: String) {
        self.level_mut(key).expect("validated level")["identifier"] = json!(name);
        let container = if let Some(world) = key.world {
            &mut self.json["worlds"][world]
        } else {
            &mut self.json
        };
        container["levels"][key.level]["identifier"] = json!(name);
    }
}
