use crate::grid_presence::{GridPresence, GridPresences};
use crate::{LevelKey, Project};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
const MAX_ACTIONS: usize = 30;
const LEVEL_STATES: usize = 40;
const LEVEL_TRIM: usize = 11;
const MAX_BYTES: usize = 64 * 1024 * 1024;
fn stub(p: &Project, key: LevelKey) -> &Value {
    match key.world {
        Some(world) => &p.json["worlds"][world]["levels"][key.level],
        None => &p.json["levels"][key.level],
    }
}
fn stub_mut(json: &mut Value, key: LevelKey) -> &mut Value {
    match key.world {
        Some(world) => &mut json["worlds"][world]["levels"][key.level],
        None => &mut json["levels"][key.level],
    }
}
fn find_level(p: &Project, iid: &str) -> Option<LevelKey> {
    p.level_keys()
        .into_iter()
        .find(|&key| p.level(key).is_some_and(|level| level["iid"] == iid))
}
struct Snapshot {
    json: Value,
    grid_presence: GridPresences,
    storage: BTreeMap<String, String>,
    base: Option<PathBuf>,
    level_iid: Option<String>,
    layer_uids: Option<BTreeSet<i64>>,
    bounds: Option<[f32; 4]>,
}
impl Snapshot {
    fn capture(p: &Project) -> Self {
        // Storage paths are publication details, not level identities. Diff the same
        // logical document whether a level is inline or has its own file.
        let mut logical = p.json.clone();
        let mut storage = BTreeMap::new();
        for key in p.level_keys() {
            if let Some(rel) = stub(p, key)["externalRelPath"].as_str()
                && let Some(level) = p.level(key)
            {
                if let Some(id) = level["iid"].as_str() {
                    storage.insert(id.into(), rel.into());
                }
                *stub_mut(&mut logical, key) = level.clone();
            }
            stub_mut(&mut logical, key)["externalRelPath"] = Value::Null;
        }
        Self {
            json: logical,
            grid_presence: p.grid_presence.clone(),
            storage,
            level_iid: None,
            layer_uids: None,
            bounds: None,
            base: p
                .path
                .as_ref()
                .and_then(|path| path.parent())
                .map(PathBuf::from),
        }
    }
    fn capture_layers(p: &Project, level_iid: &str, layer_uids: &BTreeSet<i64>) -> Self {
        let layers = find_level(p, level_iid)
            .and_then(|key| p.level(key))
            .and_then(|level| level["layerInstances"].as_array())
            .into_iter()
            .flatten()
            .filter(|layer| {
                layer["layerDefUid"]
                    .as_i64()
                    .is_some_and(|uid| layer_uids.contains(&uid))
            })
            .cloned()
            .collect::<Vec<_>>();
        let grid_presence = layers
            .iter()
            .filter_map(|layer| {
                let iid = layer["iid"].as_str()?;
                Some((iid.to_owned(), p.grid_presence.get(iid)?.clone()))
            })
            .collect();
        Self {
            json: json!({"layerInstances":layers}),
            grid_presence,
            storage: BTreeMap::new(),
            base: p
                .path
                .as_ref()
                .and_then(|path| path.parent())
                .map(PathBuf::from),
            level_iid: Some(level_iid.into()),
            layer_uids: Some(layer_uids.clone()),
            bounds: None,
        }
    }
}
#[derive(Clone)]
enum Segment {
    Key(String),
    Index(usize),
    Item(String, Value),
    Order(String),
}
struct Change {
    path: Vec<Segment>,
    before: Option<Value>,
    after: Option<Value>,
}
struct GridPresenceChange {
    iid: String,
    before: Option<GridPresence>,
    after: Option<GridPresence>,
}
struct Action {
    changes: Vec<Change>,
    grid_presence: Vec<GridPresenceChange>,
    bytes: usize,
    storage: BTreeMap<String, String>,
    base: Option<PathBuf>,
    level_iid: Option<String>,
    layer_uids: Option<BTreeSet<i64>>,
    bounds: Option<[f32; 4]>,
}
fn identity(a: &[Value], b: &[Value]) -> Option<&'static str> {
    ["iid", "uid", "defUid"].into_iter().find(|key| {
        let valid = |xs: &[Value]| {
            let mut ids = BTreeSet::new();
            xs.iter().all(|v| {
                v.get(*key)
                    .is_some_and(|id| !id.is_null() && ids.insert(id.to_string()))
            })
        };
        (!a.is_empty() || !b.is_empty()) && valid(a) && valid(b)
    })
}
fn diff(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &mut Vec<Segment>,
    out: &mut Vec<Change>,
) {
    if before == after {
        return;
    }
    match (before, after) {
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                path.push(Segment::Key(key.clone()));
                diff(a.get(key), b.get(key), path, out);
                path.pop();
            }
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            if let Some(key) = identity(a, b) {
                let ids: BTreeMap<String, Value> = a
                    .iter()
                    .chain(b)
                    .map(|v| (v[key].to_string(), v[key].clone()))
                    .collect();
                for id in ids.values() {
                    path.push(Segment::Item(key.into(), id.clone()));
                    diff(
                        a.iter().find(|v| v[key] == *id),
                        b.iter().find(|v| v[key] == *id),
                        path,
                        out,
                    );
                    path.pop();
                }
                let a_order: Vec<_> = a.iter().map(|v| v[key].clone()).collect();
                let b_order: Vec<_> = b.iter().map(|v| v[key].clone()).collect();
                if a_order != b_order {
                    path.push(Segment::Order(key.into()));
                    out.push(Change {
                        path: path.clone(),
                        before: Some(json!(a_order)),
                        after: Some(json!(b_order)),
                    });
                    path.pop();
                }
            } else if a.len() == b.len() {
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    path.push(Segment::Index(i));
                    diff(Some(a), Some(b), path, out);
                    path.pop();
                }
            } else {
                out.push(Change {
                    path: path.clone(),
                    before: before.cloned(),
                    after: after.cloned(),
                });
            }
        }
        _ => out.push(Change {
            path: path.clone(),
            before: before.cloned(),
            after: after.cloned(),
        }),
    }
}
fn matches_at(root: Option<&Value>, path: &[Segment], expected: &Option<Value>) -> bool {
    let Some((segment, rest)) = path.split_first() else {
        return root == expected.as_ref();
    };
    let child = match segment {
        Segment::Key(key) => root.and_then(|v| v.get(key)),
        Segment::Index(index) => root.and_then(|v| v.get(*index)),
        Segment::Item(key, id) => root
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|v| v[key] == *id)),
        Segment::Order(key) => {
            return root.and_then(Value::as_array).is_some_and(|a| {
                expected
                    .as_ref()
                    .and_then(Value::as_array)
                    .is_some_and(|order| a.iter().map(|v| &v[key]).eq(order.iter()))
            });
        }
    };
    matches_at(child, rest, expected)
}
fn other_level(path: &[Segment], iid: &str) -> bool {
    path.windows(2).any(|pair| matches!(pair,
        [Segment::Key(key), Segment::Item(id_key, id)] if key == "levels" && id_key == "iid" && id != iid))
}
fn current_level_path<'a>(
    path: &'a [Segment],
    world_iid: &str,
    nested: bool,
) -> Cow<'a, [Segment]> {
    match path {
        [
            Segment::Key(worlds),
            Segment::Item(key, iid),
            Segment::Key(levels),
            ..,
        ] if !nested
            && worlds == "worlds"
            && key == "iid"
            && iid == world_iid
            && levels == "levels" =>
        {
            Cow::Borrowed(&path[2..])
        }
        [Segment::Key(levels), ..] if nested && levels == "levels" => {
            let mut nested_path = vec![
                Segment::Key("worlds".into()),
                Segment::Item("iid".into(), json!(world_iid)),
            ];
            nested_path.extend_from_slice(path);
            Cow::Owned(nested_path)
        }
        _ => Cow::Borrowed(path),
    }
}
fn put(root: &mut Value, path: &[Segment], value: &Option<Value>) {
    if path.is_empty() {
        *root = value.clone().unwrap_or(Value::Null);
        return;
    }
    let (segment, rest) = path.split_first().expect("nonempty path");
    match segment {
        Segment::Key(key) => {
            if let Some(map) = root.as_object_mut() {
                if rest.is_empty() {
                    if let Some(v) = value {
                        map.insert(key.clone(), v.clone());
                    } else {
                        map.remove(key);
                    }
                } else if let Some(child) = map.get_mut(key) {
                    put(child, rest, value);
                }
            }
        }
        Segment::Index(i) => {
            if let Some(child) = root.as_array_mut().and_then(|a| a.get_mut(*i)) {
                put(child, rest, value);
            }
        }
        Segment::Item(key, id) => {
            if let Some(a) = root.as_array_mut() {
                let index = a.iter().position(|v| v[key.as_str()] == *id);
                if rest.is_empty() {
                    match (index, value) {
                        (Some(i), Some(v)) => a[i] = v.clone(),
                        (Some(i), None) => {
                            a.remove(i);
                        }
                        (None, Some(v)) => a.push(v.clone()),
                        _ => {}
                    }
                } else if let Some(i) = index {
                    put(&mut a[i], rest, value);
                }
            }
        }
        Segment::Order(key) => {
            if let (Some(a), Some(order)) = (
                root.as_array_mut(),
                value.as_ref().and_then(Value::as_array),
            ) {
                a.sort_by_key(|v| {
                    order
                        .iter()
                        .position(|id| *id == v[key.as_str()])
                        .unwrap_or(usize::MAX)
                });
            }
        }
    }
}
impl Action {
    fn between(snapshot: Snapshot, p: &Project) -> Self {
        let mut current = match (&snapshot.level_iid, &snapshot.layer_uids) {
            (Some(iid), Some(uids)) => Snapshot::capture_layers(p, iid, uids),
            _ => Snapshot::capture(p),
        };
        if let (Some(old), Some(new)) = (&current.base, &snapshot.base)
            && old != new
        {
            crate::project::rebase_paths(&mut current.json, old, new)
                .expect("saved project directories are absolute");
        }
        let mut changes = vec![];
        diff(
            Some(&snapshot.json),
            Some(&current.json),
            &mut vec![],
            &mut changes,
        );
        let grid_presence = snapshot
            .grid_presence
            .keys()
            .chain(current.grid_presence.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter_map(|iid| {
                let before = snapshot.grid_presence.get(iid);
                let after = current.grid_presence.get(iid);
                (before != after).then(|| GridPresenceChange {
                    iid: iid.clone(),
                    before: before.cloned(),
                    after: after.cloned(),
                })
            })
            .collect::<Vec<_>>();
        let mut storage = snapshot.storage;
        storage.extend(current.storage);
        let bytes = changes
            .iter()
            .map(|c| {
                c.before.as_ref().map_or(0, |v| v.to_string().len())
                    + c.after.as_ref().map_or(0, |v| v.to_string().len())
            })
            .sum::<usize>()
            + grid_presence
                .iter()
                .map(|change| {
                    change.iid.len()
                        + change.before.as_ref().map_or(0, GridPresence::bytes)
                        + change.after.as_ref().map_or(0, GridPresence::bytes)
                })
                .sum::<usize>()
            + storage
                .iter()
                .map(|(id, path)| id.len() + path.len())
                .sum::<usize>();
        Self {
            changes,
            grid_presence,
            bytes,
            storage,
            base: snapshot.base,
            level_iid: snapshot.level_iid,
            layer_uids: snapshot.layer_uids,
            bounds: snapshot.bounds,
        }
    }
    fn restore_grid_presence(&self, p: &mut Project, undo: bool, selective: bool) {
        for change in &self.grid_presence {
            let (previous, next) = if undo {
                (&change.after, &change.before)
            } else {
                (&change.before, &change.after)
            };
            if selective && p.grid_presence.get(&change.iid) != previous.as_ref() {
                continue;
            }
            if let Some(state) = next {
                p.grid_presence.insert(change.iid.clone(), state.clone());
            } else {
                p.grid_presence.remove(&change.iid);
            }
        }
    }
    fn reload_level_grid_presence(&self, p: &mut Project) {
        let Some(level) = self.level_iid.as_deref().and_then(|iid| find_level(p, iid)) else {
            return;
        };
        // LDtk's level timeline reconstructs every layer from its saved CSV.
        let mut presence = std::mem::take(&mut p.grid_presence);
        if let Some(layers) = p
            .level(level)
            .and_then(|level| level["layerInstances"].as_array())
        {
            for iid in layers.iter().filter_map(|layer| layer["iid"].as_str()) {
                presence.remove(iid);
            }
        }
        p.grid_presence = presence;
    }
    fn restore(&self, p: &mut Project, undo: bool, selective: bool) {
        if let (Some(iid), Some(uids)) = (&self.level_iid, &self.layer_uids) {
            let Some(key) = find_level(p, iid) else {
                return;
            };
            let mut current = Snapshot::capture_layers(p, iid, uids);
            if let (Some(old), Some(new)) = (&current.base, &self.base)
                && old != new
            {
                crate::project::rebase_paths(&mut current.json, old, new)
                    .expect("saved project directories are absolute");
            }
            for change in &self.changes {
                put(
                    &mut current.json,
                    &change.path,
                    if undo { &change.before } else { &change.after },
                );
            }
            if let (Some(old), Some(new)) = (&self.base, &current.base)
                && old != new
            {
                crate::project::rebase_paths(&mut current.json, old, new)
                    .expect("saved project directories are absolute");
            }
            if let Some(layers) = p
                .level_mut(key)
                .and_then(|level| level["layerInstances"].as_array_mut())
            {
                for restored in current.json["layerInstances"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                {
                    if let Some(layer) = layers
                        .iter_mut()
                        .find(|layer| layer["layerDefUid"] == restored["layerDefUid"])
                    {
                        *layer = std::mem::take(restored);
                    }
                }
            }
            self.restore_grid_presence(p, undo, selective);
            return;
        }
        let current = Snapshot::capture(p);
        self.restore_grid_presence(p, undo, selective);
        let stubs: BTreeMap<_, _> = p
            .level_keys()
            .into_iter()
            .filter_map(|key| {
                let stub = stub(p, key);
                stub["externalRelPath"].as_str()?;
                Some((stub["iid"].as_str()?.to_owned(), stub.clone()))
            })
            .collect();
        let was_external = p.json["externalLevels"].clone();
        let mut storage = self.storage.clone();
        storage.extend(current.storage.clone());
        p.json = current.json;
        if let (Some(old), Some(new)) = (&current.base, &self.base)
            && old != new
        {
            crate::project::rebase_paths(&mut p.json, old, new)
                .expect("saved project directories are absolute");
        }
        // MultiWorlds stores the dummy world's levels under a different JSON path.
        let dummy_world = p.json["dummyWorldIid"]
            .as_str()
            .filter(|_| self.level_iid.is_some());
        let worlds = p.json["worlds"]
            .as_array()
            .filter(|worlds| !worlds.is_empty());
        let level_location = match (dummy_world, worlds) {
            (Some(iid), None) => Some((iid, false)),
            (Some(iid), Some(worlds))
                if p.json["levels"].as_array().is_none_or(Vec::is_empty)
                    && worlds.iter().any(|world| world["iid"] == iid) =>
            {
                Some((iid, true))
            }
            _ => None,
        };
        // Other levels may have been edited since this level's action. Restore its
        // cross-level side effects only where those values still match that action.
        let restore = self
            .changes
            .iter()
            .map(|c| {
                let path = match level_location {
                    Some((iid, nested)) => current_level_path(&c.path, iid, nested),
                    None => Cow::Borrowed(c.path.as_slice()),
                };
                let restore = !selective
                    || self
                        .level_iid
                        .as_deref()
                        .is_none_or(|iid| !other_level(&path, iid))
                    || matches_at(
                        Some(&p.json),
                        &path,
                        if undo { &c.after } else { &c.before },
                    );
                (path, restore)
            })
            .collect::<Vec<_>>();
        for (c, (path, restore)) in self.changes.iter().zip(restore) {
            if restore {
                let value = if undo { &c.before } else { &c.after };
                put(&mut p.json, &path, value);
            }
        }
        if let (Some(old), Some(new)) = (&self.base, &current.base)
            && old != new
        {
            crate::project::rebase_paths(&mut p.json, old, new)
                .expect("saved project directories are absolute");
        }
        // A storage toggle may itself be undone. Otherwise retain the layout in
        // use now, including legacy external projects without externalLevels.
        let disabled =
            was_external != p.json["externalLevels"] && p.json["externalLevels"] == false;
        let enabled = p.json["externalLevels"] == true;
        p.external_levels.clear();
        for key in p.level_keys() {
            let level = stub(p, key).clone();
            let Some(id) = level["iid"].as_str() else {
                continue;
            };
            let rel = if disabled {
                None
            } else if enabled || current.storage.contains_key(id) {
                storage.get(id)
            } else {
                None
            };
            if let Some(rel) = rel {
                let mut stub = stubs.get(id).cloned().unwrap_or_else(|| level.clone());
                stub["externalRelPath"] = json!(rel);
                stub["layerInstances"] = Value::Null;
                *stub_mut(&mut p.json, key) = stub;
                crate::project::sync_summary(&mut p.json, key, &level);
                p.external_levels.insert(rel.clone(), level);
            } else if current.storage.contains_key(id)
                && let Some(stub) = stubs.get(id).and_then(Value::as_object)
            {
                // Unknown properties belonging only to an external summary also
                // survive when that level becomes inline.
                let target = stub_mut(&mut p.json, key);
                for (name, value) in stub {
                    if target.get(name).is_none() {
                        target[name] = value.clone();
                    }
                }
            }
        }
    }
}
#[derive(Default)]
pub struct History {
    pending: Option<Snapshot>,
    undo: Vec<Action>,
    redo: Vec<Action>,
    modified_levels: BTreeSet<String>,
}
impl History {
    /// Levels whose serialized content was invalidated since the last save.
    pub fn modified_levels(&self) -> &BTreeSet<String> {
        &self.modified_levels
    }
    pub fn mark_level_modified(&mut self, level: &Value) {
        if let Some(iid) = level["iid"].as_str() {
            self.modified_levels.insert(iid.into());
        }
    }
    pub fn clear_modified_levels(&mut self) {
        self.modified_levels.clear();
    }
    fn mark_action_levels_modified(&mut self, action: &Action) {
        if let Some(iid) = &action.level_iid {
            self.modified_levels.insert(iid.clone());
        }
        for change in &action.changes {
            for pair in change.path.windows(2) {
                if let [Segment::Key(array), Segment::Item(key, id)] = pair
                    && array == "levels"
                    && key == "iid"
                    && let Some(iid) = id.as_str()
                {
                    self.modified_levels.insert(iid.into());
                    break;
                }
            }
            if let [Segment::Key(array), Segment::Item(_, _)] = &change.path[..]
                && array == "worlds"
            {
                for world in change.before.iter().chain(&change.after) {
                    for level in world["levels"].as_array().into_iter().flatten() {
                        self.mark_level_modified(level);
                    }
                }
            }
        }
    }

    /// Source LevelTimeline bounds use raw level pixels, not renderer parallax.
    pub fn mark_rect_change(&mut self, rect: [f32; 4]) {
        let Some(snapshot) = self.pending.as_mut() else {
            return;
        };
        let [x, y, w, h] = rect;
        if let Some(bounds) = &mut snapshot.bounds {
            let right = (bounds[0] + bounds[2]).max(x + w - 1.);
            let bottom = (bounds[1] + bounds[3]).max(y + h - 1.);
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = right - bounds[0];
            bounds[3] = bottom - bounds[1];
        } else {
            snapshot.bounds = Some(rect);
        }
    }
    pub fn mark_grid_change(&mut self, p: &Project, key: LevelKey, layer: usize, cx: i32, cy: i32) {
        let Some(li) = p.layer(key, layer) else {
            return;
        };
        let grid = li["__gridSize"].as_f64().unwrap_or(16.) as f32;
        self.mark_rect_change([
            cx as f32 * grid + li["__pxTotalOffsetX"].as_f64().unwrap_or(0.) as f32,
            cy as f32 * grid + li["__pxTotalOffsetY"].as_f64().unwrap_or(0.) as f32,
            grid,
            grid,
        ]);
    }
    pub fn mark_entity_change(&mut self, p: &Project, key: LevelKey, layer: usize, entity: &Value) {
        let Some(li) = p.layer(key, layer) else {
            return;
        };
        let size = [
            entity["width"].as_f64().unwrap_or(16.) as f32,
            entity["height"].as_f64().unwrap_or(16.) as f32,
        ];
        let pos: [f32; 2] = std::array::from_fn(|axis| {
            (entity["px"][axis].as_f64().unwrap_or(0.) as f32
                - size[axis] * entity["__pivot"][axis].as_f64().unwrap_or(0.) as f32
                + 0.5)
                .floor()
                + li[if axis == 0 {
                    "__pxTotalOffsetX"
                } else {
                    "__pxTotalOffsetY"
                }]
                .as_f64()
                .unwrap_or(0.) as f32
        });
        self.mark_rect_change([pos[0], pos[1], size[0], size[1]]);
    }
    pub fn level_change_bounds(&self, p: &Project, key: LevelKey, redo: bool) -> Option<[f32; 4]> {
        let iid = p.level(key)?["iid"].as_str()?;
        (if redo { &self.redo } else { &self.undo })
            .iter()
            .rev()
            .find(|a| a.level_iid.as_deref() == Some(iid))?
            .bounds
    }
    /// A level operation may also affect reference fields, TOC or rule layers in other documents.
    pub fn begin(&mut self, p: &Project, key: LevelKey) {
        if self.pending.is_none()
            && let Some(iid) = p.level(key).and_then(|level| level["iid"].as_str())
        {
            let mut snapshot = Snapshot::capture(p);
            snapshot.level_iid = Some(iid.into());
            self.pending = Some(snapshot);
        }
    }
    /// Snapshot only manual paint data and the generated layers that painting can change.
    /// Entity editing retains the project fallback for references and TOC outside this level.
    pub fn begin_layer(&mut self, p: &Project, key: LevelKey, index: usize) {
        if self.pending.is_some() {
            return;
        }
        let Some(layer) = p.layer(key, index) else {
            return;
        };
        if !matches!(layer["__type"].as_str(), Some("IntGrid" | "Tiles")) {
            self.begin(p, key);
            return;
        }
        let (Some(uid), Some(iid)) = (
            layer["layerDefUid"].as_i64(),
            p.level(key).and_then(|level| level["iid"].as_str()),
        ) else {
            return;
        };
        let mut uids = BTreeSet::from([uid]);
        for def in p.json["defs"]["layers"].as_array().into_iter().flatten() {
            if (def["autoSourceLayerDefUid"].as_i64() == Some(uid)
                || crate::autotile::blocks_auto_layer(def, uid))
                && let Some(uid) = def["uid"].as_i64()
            {
                uids.insert(uid);
            }
        }
        self.pending = Some(Snapshot::capture_layers(p, iid, &uids));
    }
    /// Visibility changes only the addressed layer instance.
    pub fn begin_layer_visibility(&mut self, p: &Project, key: LevelKey, index: usize) {
        if self.pending.is_none()
            && let Some(uid) = p
                .layer(key, index)
                .and_then(|layer| layer["layerDefUid"].as_i64())
            && let Some(iid) = p.level(key).and_then(|level| level["iid"].as_str())
        {
            self.pending = Some(Snapshot::capture_layers(p, iid, &BTreeSet::from([uid])));
        }
    }

    pub fn begin_project(&mut self, p: &Project) {
        if self.pending.is_none() {
            self.pending = Some(Snapshot::capture(p));
        }
    }
    pub fn commit(&mut self, p: &Project) -> bool {
        let Some(snapshot) = self.pending.take() else {
            return false;
        };
        let action = Action::between(snapshot, p);
        if action.changes.is_empty() && action.grid_presence.is_empty() {
            return false;
        }
        self.mark_action_levels_modified(&action);
        if action.level_iid.is_some() {
            self.redo
                .retain(|previous| previous.level_iid != action.level_iid);
        } else {
            self.redo.clear();
        }
        let scope = action.level_iid.clone();
        if scope.is_none() && action.bytes > MAX_BYTES {
            self.undo.retain(|previous| previous.level_iid.is_some());
            return true;
        }
        self.undo.push(action);
        let count = self.undo.iter().filter(|a| a.level_iid == scope).count();
        if scope.is_some() {
            if count >= LEVEL_STATES {
                for _ in 0..LEVEL_TRIM {
                    if let Some(index) = self.undo.iter().position(|a| a.level_iid == scope) {
                        self.undo.remove(index);
                    }
                }
            }
        } else {
            while self.undo.iter().filter(|a| a.level_iid.is_none()).count() > MAX_ACTIONS
                || self
                    .undo
                    .iter()
                    .filter(|a| a.level_iid.is_none())
                    .map(|a| a.bytes)
                    .sum::<usize>()
                    > MAX_BYTES
            {
                let index = self
                    .undo
                    .iter()
                    .position(|a| a.level_iid.is_none())
                    .expect("structural action");
                self.undo.remove(index);
            }
        }
        true
    }
    pub fn cancel(&mut self, p: &mut Project) -> bool {
        let Some(snapshot) = self.pending.take() else {
            return false;
        };
        Action::between(snapshot, p).restore(p, true, false);
        true
    }
    pub fn undo(&mut self, p: &mut Project) -> bool {
        self.pending = None;
        let Some(a) = self.undo.pop() else {
            return false;
        };
        a.restore(p, true, false);
        a.reload_level_grid_presence(p);
        self.mark_action_levels_modified(&a);
        self.redo.push(a);
        true
    }
    pub fn redo(&mut self, p: &mut Project) -> bool {
        self.pending = None;
        let Some(a) = self.redo.pop() else {
            return false;
        };
        a.restore(p, false, false);
        a.reload_level_grid_presence(p);
        self.mark_action_levels_modified(&a);
        self.undo.push(a);
        true
    }
    /// Undo the selected level's latest edit even when another level was edited later.
    pub fn undo_level(&mut self, p: &mut Project, key: LevelKey) -> bool {
        let iid = p.level(key).and_then(|level| level["iid"].as_str());
        let Some(index) = self
            .undo
            .iter()
            .rposition(|a| a.level_iid.as_deref() == iid && iid.is_some())
        else {
            return false;
        };
        self.pending = None;
        let action = self.undo.remove(index);
        if action.layer_uids.is_some() {
            action.restore(p, true, true);
        } else if p
            .edit_document(|p| {
                action.restore(p, true, true);
                p.refresh_derived_data()
            })
            .is_err()
        {
            self.undo.insert(index, action);
            return false;
        }
        action.reload_level_grid_presence(p);
        self.mark_action_levels_modified(&action);
        self.redo.push(action);
        true
    }
    pub fn redo_level(&mut self, p: &mut Project, key: LevelKey) -> bool {
        let iid = p.level(key).and_then(|level| level["iid"].as_str());
        let Some(index) = self
            .redo
            .iter()
            .rposition(|a| a.level_iid.as_deref() == iid && iid.is_some())
        else {
            return false;
        };
        self.pending = None;
        let action = self.redo.remove(index);
        if action.layer_uids.is_some() {
            action.restore(p, false, true);
        } else if p
            .edit_document(|p| {
                action.restore(p, false, true);
                p.refresh_derived_data()
            })
            .is_err()
        {
            self.redo.insert(index, action);
            return false;
        }
        action.reload_level_grid_presence(p);
        self.mark_action_levels_modified(&action);
        self.undo.push(action);
        true
    }
    /// Apply structural-event invalidation after finishing any active gesture.
    /// Current document contents and structural project undo/redo remain intact.
    pub fn clear_level_timelines(&mut self) {
        self.undo.retain(|action| action.level_iid.is_none());
        self.redo.retain(|action| action.level_iid.is_none());
        if self
            .pending
            .as_ref()
            .is_some_and(|snapshot| snapshot.level_iid.is_some())
        {
            self.pending = None;
        }
    }
    pub fn can_undo_level(&self, p: &Project, key: LevelKey) -> bool {
        let iid = p.level(key).and_then(|level| level["iid"].as_str());
        iid.is_some() && self.undo.iter().any(|a| a.level_iid.as_deref() == iid)
    }
    pub fn can_redo_level(&self, p: &Project, key: LevelKey) -> bool {
        let iid = p.level(key).and_then(|level| level["iid"].as_str());
        iid.is_some() && self.redo.iter().any(|a| a.level_iid.as_deref() == iid)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
