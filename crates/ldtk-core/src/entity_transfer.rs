use super::*;
struct Transfer {
    key: LevelKey,
    source: usize,
    target: usize,
    indices: Vec<usize>,
}
fn compatible(entity: &Value, layer: &Value) -> bool {
    let tags = entity["tags"].as_array();
    let matches = |key| {
        layer[key].as_array().is_some_and(|wanted| {
            wanted
                .iter()
                .any(|tag| tags.is_some_and(|tags| tags.contains(tag)))
        })
    };
    !matches("excludedTags")
        && (layer["requiredTags"].as_array().is_none_or(Vec::is_empty) || matches("requiredTags"))
}
fn plan(
    project: &Project,
    source: i64,
    target: i64,
    filter: &str,
    mut transfers: Option<&mut Vec<Transfer>>,
) -> Result<usize> {
    if source == target {
        return Err(invalid("origin and destination layers must differ"));
    }
    for uid in [source, target] {
        if project
            .layer_def(uid)
            .is_none_or(|d| d["type"] != "Entities")
        {
            return Err(invalid("entity transfer requires two Entities layers"));
        }
    }
    let target_def = project.layer_def(target).expect("validated layer");
    let (kind, value) = filter
        .split_once('.')
        .ok_or_else(|| invalid("choose All entities, an identifier, or a tag"))?;
    if !["all", "id", "tag"].contains(&kind) || value.is_empty() {
        return Err(invalid("invalid entity transfer filter"));
    }
    let entities = project.json["defs"]["entities"]
        .as_array()
        .ok_or_else(|| invalid("entity definitions missing"))?;
    if kind == "id" {
        let entity = entities
            .iter()
            .find(|d| d["identifier"] == value)
            .ok_or_else(|| invalid("entity identifier not found"))?;
        if !compatible(entity, target_def) {
            return Err(invalid(
                "destination layer does not accept this entity's tags",
            ));
        }
    }
    let mut selected_by_uid = HashMap::with_capacity(entities.len());
    for entity in entities {
        selected_by_uid.entry(&entity["uid"]).or_insert_with(|| {
            // The original tag filter bypasses the destination layer's tag constraints.
            match kind {
                "all" => compatible(entity, target_def),
                "id" => entity["identifier"] == value,
                "tag" => entity["tags"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag == value)),
                _ => unreachable!("validated filter kind"),
            }
        });
    }
    let mut count = 0;
    for key in project.iter_level_keys() {
        let level = project
            .level(key)
            .ok_or_else(|| invalid("every external level must be loaded"))?;
        let layers = level["layerInstances"]
            .as_array()
            .ok_or_else(|| invalid("level layers missing"))?;
        let Some(source_index) = layers.iter().position(|l| l["layerDefUid"] == source) else {
            continue;
        };
        let source_entities = layers[source_index]["entityInstances"]
            .as_array()
            .ok_or_else(|| invalid("origin entity array missing"))?;
        let mut indices = Vec::new();
        let previously_matched = count;
        for (index, instance) in source_entities.iter().enumerate() {
            let selected = selected_by_uid
                .get(&instance["defUid"])
                .ok_or_else(|| invalid("entity definition missing"))?;
            if *selected {
                count += 1;
                if transfers.is_some() {
                    indices.push(index);
                }
            }
        }
        if count == previously_matched {
            continue;
        }
        let target_index = layers
            .iter()
            .position(|l| l["layerDefUid"] == target)
            .ok_or_else(|| invalid("destination layer instance missing"))?;
        if !layers[target_index]["entityInstances"].is_array() {
            return Err(invalid("destination entity array missing"));
        }
        if let Some(transfers) = &mut transfers {
            transfers.push(Transfer {
                key,
                source: source_index,
                target: target_index,
                indices,
            });
        }
    }
    Ok(count)
}
impl Project {
    /// Source filter grammar: `all.*`, `id.EntityIdentifier`, or `tag.entityTag`.
    /// Moves existing instances in every world/level, retaining their IIDs and field data.
    pub fn move_entities_between_layers(
        &mut self,
        source: i64,
        target: i64,
        filter: &str,
    ) -> Result<usize> {
        let mut transfers = Vec::new();
        let count = plan(self, source, target, filter, Some(&mut transfers))?;
        if count == 0 {
            return Ok(0);
        }
        self.edit_document(|p| {
            for transfer in transfers {
                let layers = p.level_mut(transfer.key).expect("validated level")["layerInstances"]
                    .as_array_mut()
                    .expect("validated layers");
                let entities = std::mem::take(
                    layers[transfer.source]["entityInstances"]
                        .as_array_mut()
                        .expect("validated origin"),
                );
                let (mut moved, mut retained) = (Vec::new(), Vec::new());
                let mut selected = transfer.indices.into_iter().peekable();
                for (index, entity) in entities.into_iter().enumerate() {
                    if selected.peek() == Some(&index) {
                        selected.next();
                        moved.push(entity);
                    } else {
                        retained.push(entity);
                    }
                }
                layers[transfer.source]["entityInstances"] = Value::Array(retained);
                layers[transfer.target]["entityInstances"]
                    .as_array_mut()
                    .expect("validated destination")
                    .extend(moved);
            }
            p.refresh_derived_data()?;
            Ok(count)
        })
    }
    pub fn count_entities_to_transfer(
        &self,
        source: i64,
        target: i64,
        filter: &str,
    ) -> Result<usize> {
        plan(self, source, target, filter, None)
    }
}
