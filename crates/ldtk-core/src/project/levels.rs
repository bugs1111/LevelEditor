use super::Project;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LevelKey {
    pub world: Option<usize>,
    pub level: usize,
}

/// A borrowed entity and its position in the project document.
pub struct EntityMatch<'a> {
    pub level: LevelKey,
    pub layer_index: usize,
    pub entity_index: usize,
    pub entity: &'a Value,
}

impl Project {
    /// Snapshot the keys when an operation needs to mutate the project during traversal.
    pub fn level_keys(&self) -> Vec<LevelKey> {
        self.iter_level_keys().collect()
    }

    /// Visit root levels first, then each world's levels in document order.
    pub fn iter_level_keys(&self) -> impl Iterator<Item = LevelKey> + '_ {
        self.world_indices()
            .flat_map(|world| self.level_keys_in_world(world))
    }

    pub(crate) fn world_indices(&self) -> impl Iterator<Item = Option<usize>> + use<> {
        let count = self.json["worlds"].as_array().map_or(0, Vec::len);
        std::iter::once(None).chain((0..count).map(Some))
    }

    /// Only the length is captured, so callers can edit levels while walking this world.
    pub fn level_keys_in_world(
        &self,
        world: Option<usize>,
    ) -> impl ExactSizeIterator<Item = LevelKey> + DoubleEndedIterator + use<> {
        let count = self.level_stubs_in_world(world).len();
        (0..count).map(move |level| LevelKey { world, level })
    }

    fn level_stubs_in_world(&self, world: Option<usize>) -> &[Value] {
        let container = match world {
            Some(index) => self.json["worlds"].get(index),
            None => Some(&self.json),
        };
        container
            .and_then(|c| c["levels"].as_array())
            .map_or(&[], Vec::as_slice)
    }

    /// Visit available levels in document order, retaining the keys of external levels.
    pub fn levels_in_world(
        &self,
        world: Option<usize>,
    ) -> impl DoubleEndedIterator<Item = (LevelKey, &Value)> {
        self.level_stubs_in_world(world)
            .iter()
            .enumerate()
            .filter_map(move |(level, stub)| {
                self.resolve_level_stub(stub)
                    .map(|data| (LevelKey { world, level }, data))
            })
    }

    /// Missing external documents retain their slot so each caller can report the load error.
    pub(crate) fn positioned_levels(
        &self,
        world: Option<usize>,
    ) -> impl ExactSizeIterator<Item = Option<(&Value, [i64; 2])>> {
        let container = match world {
            Some(index) => &self.json["worlds"][index],
            None => &self.json,
        };
        let layout = container["worldLayout"].as_str().unwrap_or("Free");
        let mut cursor = 0_i64;
        self.level_keys_in_world(world).map(move |key| {
            let level = self.level(key)?;
            let position = if layout.starts_with("Linear") {
                let horizontal = layout == "LinearHorizontal";
                let origin = cursor;
                let extent = level[if horizontal { "pxWid" } else { "pxHei" }]
                    .as_i64()
                    .unwrap_or(0);
                cursor = cursor.saturating_add(extent).saturating_add(48);
                if horizontal { [origin, 0] } else { [0, origin] }
            } else {
                [
                    level["worldX"].as_i64().unwrap_or(0),
                    level["worldY"].as_i64().unwrap_or(0),
                ]
            };
            Some((level, position))
        })
    }

    pub fn level_by_iid(&self, iid: &str) -> Option<(LevelKey, &Value)> {
        self.iter_level_keys().find_map(|key| {
            let level = self.level(key)?;
            (level["iid"] == iid).then_some((key, level))
        })
    }

    pub fn entity_by_iid(&self, iid: &str) -> Option<EntityMatch<'_>> {
        self.iter_entities()
            .find(|found| found.entity["iid"] == iid)
    }

    /// Visit entities in level, layer and instance order, resolving external levels.
    pub fn iter_entities(&self) -> impl Iterator<Item = EntityMatch<'_>> {
        self.iter_level_keys().flat_map(|level| {
            self.level(level)
                .and_then(|level| level["layerInstances"].as_array())
                .into_iter()
                .flatten()
                .enumerate()
                .flat_map(move |(layer_index, layer)| {
                    layer["entityInstances"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(move |(entity_index, entity)| EntityMatch {
                            level,
                            layer_index,
                            entity_index,
                            entity,
                        })
                })
        })
    }

    pub(super) fn level_stub(&self, key: LevelKey) -> Option<&Value> {
        let container = match key.world {
            Some(world) => self.json.get("worlds")?.get(world)?,
            None => &self.json,
        };
        container.get("levels")?.get(key.level)
    }

    pub(super) fn level_stub_mut(&mut self, key: LevelKey) -> Option<&mut Value> {
        let container = match key.world {
            Some(world) => self.json.get_mut("worlds")?.get_mut(world)?,
            None => &mut self.json,
        };
        container.get_mut("levels")?.get_mut(key.level)
    }

    pub fn level(&self, key: LevelKey) -> Option<&Value> {
        self.resolve_level_stub(self.level_stub(key)?)
    }

    fn resolve_level_stub<'a>(&'a self, stub: &'a Value) -> Option<&'a Value> {
        match stub.get("externalRelPath").and_then(Value::as_str) {
            Some(rel) => self.external_levels.get(rel),
            None => Some(stub),
        }
    }

    pub fn level_mut(&mut self, key: LevelKey) -> Option<&mut Value> {
        let rel = self
            .level_stub(key)?
            .get("externalRelPath")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(rel) = rel {
            return self.external_levels.get_mut(&rel);
        }
        self.level_stub_mut(key)
    }

    pub(crate) fn set_level_name(&mut self, key: LevelKey, name: String) {
        if self.level(key).expect("validated level")["identifier"] != name {
            self.level_mut(key).expect("validated level")["identifier"] =
                Value::String(name.clone());
        }
        let stub = self.level_stub_mut(key).expect("validated level");
        if stub["identifier"] != name {
            stub["identifier"] = Value::String(name);
        }
    }

    pub fn layer(&self, key: LevelKey, layer: usize) -> Option<&Value> {
        self.level(key)?.get("layerInstances")?.get(layer)
    }
    pub fn layer_mut(&mut self, key: LevelKey, layer: usize) -> Option<&mut Value> {
        self.level_mut(key)?
            .get_mut("layerInstances")?
            .get_mut(layer)
    }
    pub fn layer_def(&self, uid: i64) -> Option<&Value> {
        self.definition("layers", uid)
    }
    pub fn tileset(&self, uid: i64) -> Option<&Value> {
        self.definition("tilesets", uid)
    }
    pub(crate) fn definition(&self, kind: &str, uid: i64) -> Option<&Value> {
        self.json
            .get("defs")?
            .get(kind)?
            .as_array()?
            .iter()
            .find(|value| value.get("uid").and_then(Value::as_i64) == Some(uid))
    }
}
