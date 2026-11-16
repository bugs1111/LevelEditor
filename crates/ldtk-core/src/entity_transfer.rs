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
fn plan(project: &Project, source: i64, target: i64, filter: &str) -> Result<Vec<Transfer>> {
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
    let mut transfers = Vec::new();
    for key in project.level_keys() {
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
        for (index, instance) in source_entities.iter().enumerate() {
            let entity = entities
                .iter()
                .find(|d| d["uid"] == instance["defUid"])
                .ok_or_else(|| invalid("entity definition missing"))?;
            // The original tag-filter branch intentionally does not apply destination-tag filtering.
            let selected = match kind {
                "all" => compatible(entity, target_def),
                "id" => entity["identifier"] == value,
                "tag" => entity["tags"]
                    .as_array()
                    .is_some_and(|tags| tags.iter().any(|tag| tag == value)),
                _ => false,
            };
            if selected {
                indices.push(index);
            }
        }
        if indices.is_empty() {
            continue;
        }
        let target_index = layers
            .iter()
            .position(|l| l["layerDefUid"] == target)
            .ok_or_else(|| invalid("destination layer instance missing"))?;
        if !layers[target_index]["entityInstances"].is_array() {
            return Err(invalid("destination entity array missing"));
        }
        transfers.push(Transfer {
            key,
            source: source_index,
            target: target_index,
            indices,
        });
    }
    Ok(transfers)
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
        self.edit_document(|p| {
            let transfers = plan(p, source, target, filter)?;
            let count = transfers.iter().map(|t| t.indices.len()).sum();
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
                layers[transfer.source]["entityInstances"] = json!(retained);
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
        Ok(plan(self, source, target, filter)?
            .iter()
            .map(|t| t.indices.len())
            .sum())
    }
}
