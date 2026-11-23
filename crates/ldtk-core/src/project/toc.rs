//! Entity table of contents for document edits, saves, and crash recovery.
use super::Project;
use crate::{Result, workflow::invalid};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

struct TocEntry<'a> {
    row: Value,
    field_uids: HashSet<&'a Value>,
    instances: Vec<Value>,
    data: Vec<Value>,
}

impl<'a> TocEntry<'a> {
    fn new(definition: &'a Value, previous: Option<&Value>) -> Self {
        // Keep extension members and key order without copying stale instance lists.
        let mut row = json!({});
        if let Some(object) = previous.and_then(Value::as_object) {
            for (key, value) in object {
                row[key] = match key.as_str() {
                    "instances" | "instancesData" => Value::Null,
                    _ => value.clone(),
                };
            }
        }
        row["identifier"] = definition["identifier"].clone();
        let field_uids = definition["fieldDefs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| field["exportToToc"] == true)
            .map(|field| &field["uid"])
            .collect();
        Self {
            row,
            field_uids,
            instances: Vec::new(),
            data: Vec::new(),
        }
    }

    fn add(
        &mut self,
        entity: &Value,
        layer: &Value,
        level: &Value,
        world_iid: &Value,
        [x, y]: [i64; 2],
        legacy: bool,
    ) {
        let mut item = json!({
            "iids": {
                "entityIid": entity["iid"], "layerIid": layer["iid"],
                "levelIid": level["iid"], "worldIid": world_iid,
            },
            "worldX": x + entity["px"][0].as_i64().unwrap_or(0),
            "worldY": y + entity["px"][1].as_i64().unwrap_or(0),
            "widPx": entity["width"], "heiPx": entity["height"],
        });
        if legacy {
            self.instances.push(item["iids"].clone());
        }
        let mut fields = serde_json::Map::new();
        for field in entity["fieldInstances"].as_array().into_iter().flatten() {
            if self.field_uids.contains(&field["defUid"])
                && let Some(name) = field["__identifier"].as_str()
            {
                fields.insert(name.into(), field["__value"].clone());
            }
        }
        item["fields"] = Value::Object(fields);
        self.data.push(item);
    }

    fn finish(mut self) -> Value {
        self.row["instances"] = Value::Array(self.instances);
        self.row["instancesData"] = Value::Array(self.data);
        self.row
    }
}

impl Project {
    pub(crate) fn refresh_table_of_contents(&mut self) -> Result<()> {
        let old_toc = self.json["toc"].as_array();
        let mut previous_by_name = HashMap::new();
        for row in old_toc.into_iter().flatten() {
            previous_by_name.entry(&row["identifier"]).or_insert(row);
        }
        let mut entries = Vec::new();
        let mut by_uid = HashMap::<&Value, Vec<usize>>::new();
        for def in self.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|def| def["exportToToc"] == true)
        {
            let previous = previous_by_name.get(&def["identifier"]).copied();
            by_uid.entry(&def["uid"]).or_default().push(entries.len());
            entries.push(TocEntry::new(def, previous));
        }
        if entries.is_empty() {
            if old_toc.is_some() || self.json["jsonVersion"] == "1.5.4" {
                self.json["toc"] = Value::Array(Vec::new());
            }
            return Ok(());
        }
        let legacy = self.json["flags"].as_array().is_some_and(|flags| {
            flags
                .iter()
                .any(|flag| flag == "ExportOldTableOfContentData")
        });
        for world in self.world_indices() {
            let world_iid = match world {
                Some(index) => &self.json["worlds"][index]["iid"],
                None => &self.json["dummyWorldIid"],
            };
            for level in self.positioned_levels(world) {
                let (level, position) =
                    level.ok_or_else(|| invalid("all external levels must be loaded"))?;
                for layer in level["layerInstances"].as_array().into_iter().flatten() {
                    for entity in layer["entityInstances"].as_array().into_iter().flatten() {
                        if let Some(indices) = by_uid.get(&entity["defUid"]) {
                            for &index in indices {
                                entries[index]
                                    .add(entity, layer, level, world_iid, position, legacy);
                            }
                        }
                    }
                }
            }
        }
        self.json["toc"] = Value::Array(entries.into_iter().map(TocEntry::finish).collect());
        Ok(())
    }
}
