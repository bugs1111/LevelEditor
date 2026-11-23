//! LDtk's deterministic auto-layer evaluator. Rule order is evaluation order;
//! serialized tiles use its reverse (painter's order), then ascending cell IDs.
use crate::{Error, LevelKey, Project, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[path = "autotile_edit.rs"]
mod editing;
pub(crate) use editing::clipboard_rule;
#[path = "autotile_noise.rs"]
mod noise;

const MAX_PATTERN_SIZE: i32 = 9;

fn rule_pattern(rule: &Value) -> Option<(usize, &[Value])> {
    let size = usize::try_from(rule["size"].as_u64()?).ok()?;
    let pattern = rule["pattern"].as_array()?;
    (size <= MAX_PATTERN_SIZE as usize
        && size % 2 == 1
        && pattern.len() == size * size
        && pattern.iter().all(Value::is_i64))
    .then_some((size, pattern))
}

/// Whether mirroring the pattern on one axis leaves every condition unchanged.
pub fn auto_rule_pattern_is_symmetric(rule: &Value, horizontal: bool) -> bool {
    let Some((size, pattern)) = rule_pattern(rule) else {
        return false;
    };
    (0..size).all(|y| {
        (0..size).all(|x| {
            let mirrored = if horizontal {
                y * size + (size - 1 - x)
            } else {
                (size - 1 - y) * size + x
            };
            pattern[y * size + x] == pattern[mirrored]
        })
    })
}

/// Remove matching empty borders from a rule while keeping its center fixed.
pub fn trim_auto_rule_pattern(rule: &mut Value) {
    let Some((old_size, pattern)) = rule_pattern(rule) else {
        return;
    };
    let center = old_size / 2;
    let radius = pattern
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0)
        .map(|(index, _)| {
            (index % old_size)
                .abs_diff(center)
                .max((index / old_size).abs_diff(center))
        })
        .max()
        .unwrap_or(0);
    let size = 2 * radius + 1;
    if size < old_size {
        let inset = center - radius;
        let trimmed = pattern
            .chunks_exact(old_size)
            .skip(inset)
            .take(size)
            .flat_map(|row| row[inset..inset + size].iter().cloned())
            .collect();
        rule["size"] = json!(size);
        rule["pattern"] = Value::Array(trimmed);
    }
}

impl Project {
    pub(crate) fn auto_rule_source_values(&self, layer: &Value) -> BTreeSet<i64> {
        let source = linked_rule_source_uid(layer)
            .and_then(|uid| self.layer_def(uid))
            .unwrap_or(layer);
        int_grid_value_ids(source)
    }

    pub(crate) fn tidy_auto_rules_for_save(&mut self) {
        let Some(layers) = self
            .json
            .pointer_mut("/defs/layers")
            .and_then(Value::as_array_mut)
        else {
            return;
        };
        let grid_values: BTreeMap<_, BTreeSet<_>> = layers
            .iter()
            .filter_map(|layer| {
                let uid = layer["uid"].as_i64()?;
                Some((uid, int_grid_value_ids(layer)))
            })
            .collect();
        for layer in layers {
            let source = linked_rule_source_uid(layer)
                .and_then(|uid| grid_values.get(&uid))
                .or_else(|| layer["uid"].as_i64().and_then(|uid| grid_values.get(&uid)));
            tidy_layer_auto_rules(layer, source);
        }
    }
}

fn linked_rule_source_uid(layer: &Value) -> Option<i64> {
    (layer["type"] == "AutoLayer")
        .then(|| layer["autoSourceLayerDefUid"].as_i64())
        .flatten()
}

fn int_grid_value_ids(layer: &Value) -> BTreeSet<i64> {
    arr(&layer["intGridValues"])
        .iter()
        .filter_map(|value| value["value"].as_i64())
        .collect()
}

pub(crate) fn tidy_layer_auto_rules(layer: &mut Value, source: Option<&BTreeSet<i64>>) {
    for group in layer
        .get_mut("autoRuleGroups")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        tidy_auto_rule_group(group, source);
    }
}

pub(crate) fn tidy_auto_rule_group(group: &mut Value, source: Option<&BTreeSet<i64>>) {
    for rule in group
        .get_mut("rules")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        tidy_rule(rule, source);
    }
}

pub(crate) fn tidy_rule(rule: &mut Value, source: Option<&BTreeSet<i64>>) {
    trim_auto_rule_pattern(rule);
    for (key, horizontal) in [("flipX", true), ("flipY", false)] {
        if rule[key] == true && auto_rule_pattern_is_symmetric(rule, horizontal) {
            rule[key] = Value::Bool(false);
        }
    }
    let x_modulo = rule["xModulo"].as_i64().unwrap_or(1);
    let y_modulo = rule["yModulo"].as_i64().unwrap_or(1);
    let checker = match (rule["checker"].as_str(), x_modulo, y_modulo) {
        (Some("Horizontal"), 1, y) => Some(if y > 1 { "Vertical" } else { "None" }),
        (Some("Vertical"), x, 1) => Some(if x > 1 { "Horizontal" } else { "None" }),
        _ => None,
    };
    if let Some(checker) = checker {
        rule["checker"] = json!(checker);
    }
    if let Some(value) = rule["outOfBoundsValue"].as_i64()
        && value != 0
        && source.is_none_or(|values| !values.contains(&value))
    {
        rule["outOfBoundsValue"] = Value::Null;
    }
}

fn int(v: &Value, k: &str, default: i32) -> i32 {
    v[k].as_i64().map_or(default, |n| n as i32)
}
fn float(v: &Value, k: &str, default: f64) -> f64 {
    v[k].as_f64().unwrap_or(default)
}
fn flag(v: &Value, k: &str, default: bool) -> bool {
    v[k].as_bool().unwrap_or(default)
}

/// Per-cell Perlin value shared by auto-layer evaluation and the editor preview.
pub fn auto_rule_perlin_value(layer_seed: i32, cell: [i32; 2], rule: &Value) -> f64 {
    let scale = float(rule, "perlinScale", 0.2);
    noise::perlin(
        f64::from(layer_seed) + f64::from(int(rule, "perlinSeed", 0)),
        f64::from(cell[0]) * scale,
        f64::from(cell[1]) * scale,
        int(rule, "perlinOctaves", 2),
    )
}
fn arr(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}
fn number(n: f64) -> Value {
    if n.fract() == 0. && n.abs() < i64::MAX as f64 {
        json!(n as i64)
    } else {
        json!(n)
    }
}
pub(crate) fn blocks_auto_layer(def: &Value, uid: i64) -> bool {
    match def
        .get("layerUidsPreventingAutoTilingHere")
        .and_then(Value::as_array)
    {
        Some(uids) => uids.iter().any(|v| v.as_i64() == Some(uid)),
        None => def["autoTilesKilledByOtherLayerUid"].as_i64() == Some(uid),
    }
}
fn find_uid(values: &Value, uid: i64) -> Option<&Value> {
    arr(values).iter().find(|v| v["uid"].as_i64() == Some(uid))
}

/// Collects generated updates for one level while a brush or its cooldown is active.
#[derive(Default)]
pub struct DeferredAutoLayers {
    targets: BTreeMap<i64, Option<[i32; 4]>>,
}

impl DeferredAutoLayers {
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    fn merge(&mut self, uid: i64, rect: [i32; 4]) {
        self.targets
            .entry(uid)
            .and_modify(|old| {
                if let Some(old) = old {
                    let left = old[0].min(rect[0]);
                    let top = old[1].min(rect[1]);
                    let right = (old[0] + old[2]).max(rect[0] + rect[2]);
                    let bottom = (old[1] + old[3]).max(rect[1] + rect[3]);
                    *old = [left, top, right - left, bottom - top];
                }
            })
            .or_insert(Some(rect));
    }
}

fn auto_render_is_deferred(project: &Project, def: &Value, source: Option<i64>) -> bool {
    def["useAsyncRender"] == true
        || source
            .and_then(|uid| project.layer_def(uid))
            .is_some_and(|ld| ld["useAsyncRender"] == true)
}

impl Project {
    /// Update immediate targets and queue only the opted-in generated targets.
    pub(crate) fn update_auto_layers_after_paint(
        &mut self,
        key: LevelKey,
        painted_uid: i64,
        rect: [i32; 4],
        pending: Option<&mut DeferredAutoLayers>,
    ) -> Result<()> {
        let Some(pending) = pending else {
            return self.rebuild_auto_layers_area(key, painted_uid, rect);
        };
        let mut delayed = Vec::new();
        self.rebuild_auto_selected(key, |project, uid, def, source| {
            if source != Some(painted_uid) && !blocks_auto_layer(def, painted_uid) {
                return None;
            }
            if auto_render_is_deferred(project, def, source) {
                delayed.push(uid);
                None
            } else {
                Some(Some(rect))
            }
        })?;
        for uid in delayed {
            pending.merge(uid, rect);
        }
        Ok(())
    }

    pub(crate) fn update_auto_layers_after_selection(
        &mut self,
        key: LevelKey,
        changed: &BTreeSet<i64>,
        pending: &mut DeferredAutoLayers,
    ) -> Result<()> {
        let mut delayed = Vec::new();
        self.rebuild_auto_selected(key, |project, uid, def, source| {
            if !source.is_some_and(|uid| changed.contains(&uid))
                && !changed.iter().any(|&uid| blocks_auto_layer(def, uid))
            {
                return None;
            }
            if auto_render_is_deferred(project, def, source) {
                delayed.push(uid);
                None
            } else {
                Some(None)
            }
        })?;
        pending
            .targets
            .extend(delayed.into_iter().map(|uid| (uid, None)));
        Ok(())
    }

    /// Settle queued targets atomically before publishing a stroke or document snapshot.
    pub fn settle_auto_layers(
        &mut self,
        key: LevelKey,
        pending: &mut DeferredAutoLayers,
    ) -> Result<()> {
        self.rebuild_auto_selected(key, |_, uid, _, _| pending.targets.get(&uid).copied())?;
        pending.targets.clear();
        Ok(())
    }

    /// Regenerate every auto layer in a level, committing only after all layers succeed.
    pub fn rebuild_auto_layers(&mut self, key: LevelKey) -> Result<()> {
        self.rebuild_auto(key, None)
    }
    pub fn rebuild_auto_layer(&mut self, key: LevelKey, uid: i64) -> Result<()> {
        self.rebuild_auto_filtered(key, None, Some(uid))
    }
    pub fn randomize_auto_layer(&mut self, key: LevelKey, index: usize) -> Result<()> {
        self.edit_document(|p| {
            let layer = p.layer_mut(key, index)
                .ok_or_else(|| Error::Invalid("Layer instance not found".into()))?;
            let uid = layer["layerDefUid"].as_i64()
                .ok_or_else(|| Error::Invalid("Layer definition not found".into()))?;
            layer["seed"] = json!((uuid::Uuid::new_v4().as_u128() % 9_999_999) as u32);
            p.rebuild_auto_layer(key, uid)
        })
    }
    /// Re-evaluate cells affected by a source IntGrid rectangle [x,y,width,height].
    /// Existing cache entries outside the expanded rule footprint are preserved.
    pub fn rebuild_auto_layers_area(
        &mut self,
        key: LevelKey,
        source_uid: i64,
        rect: [i32; 4],
    ) -> Result<()> {
        if rect[2] < 0 || rect[3] < 0 {
            return Err(Error::Invalid(
                "negative auto-layer update rectangle".into(),
            ));
        }
        if rect[2] == 0 || rect[3] == 0 {
            return Ok(());
        }
        self.rebuild_auto(key, Some((source_uid, rect)))
    }
    fn rebuild_auto(&mut self, key: LevelKey, area: Option<(i64, [i32; 4])>) -> Result<()> {
        self.rebuild_auto_filtered(key, area, None)
    }
    fn rebuild_auto_filtered(
        &mut self,
        key: LevelKey,
        area: Option<(i64, [i32; 4])>,
        only_uid: Option<i64>,
    ) -> Result<()> {
        self.rebuild_auto_selected(key, |_, uid, def, source| {
            if only_uid.is_some_and(|only| uid != only)
                || area.is_some_and(|(painted, _)| {
                    source != Some(painted) && !blocks_auto_layer(def, painted)
                })
            {
                return None;
            }
            Some(area.map(|(_, rect)| rect))
        })
    }

    fn rebuild_auto_selected(
        &mut self,
        key: LevelKey,
        mut select: impl FnMut(&Project, i64, &Value, Option<i64>) -> Option<Option<[i32; 4]>>,
    ) -> Result<()> {
        let level = self
            .level(key)
            .ok_or_else(|| Error::Invalid("missing level".into()))?;
        let layers = level["layerInstances"]
            .as_array()
            .ok_or_else(|| Error::Invalid("missing layer instances".into()))?;
        let mut updates = Vec::new();
        for (index, li) in layers.iter().enumerate() {
            let Some(uid) = li["layerDefUid"].as_i64() else {
                continue;
            };
            let Some(ld) = find_uid(&self.json["defs"]["layers"], uid) else {
                continue;
            };
            let kind = ld["type"].as_str().unwrap_or("");
            let tileset_uid = ld["tilesetDefUid"]
                .as_i64()
                .or_else(|| ld["autoTilesetDefUid"].as_i64());
            if kind != "AutoLayer" && !(kind == "IntGrid" && tileset_uid.is_some()) {
                continue;
            }
            let source_uid = if kind == "IntGrid" {
                ld["uid"].as_i64()
            } else {
                ld["autoSourceLayerDefUid"].as_i64()
            };
            let Some(rect) = select(self, uid, ld, source_uid) else {
                continue;
            };
            let source = layers
                .iter()
                .find(|l| l["layerDefUid"].as_i64() == source_uid);
            let Some(source) = source.filter(|_| tileset_uid.is_some()) else {
                updates.push((index, json!([])));
                continue;
            };
            let source_def = find_uid(&self.json["defs"]["layers"], source_uid.unwrap())
                .ok_or_else(|| Error::Invalid("missing auto-layer source definition".into()))?;
            let atlas_uid = li["overrideTilesetUid"].as_i64().or(tileset_uid).unwrap();
            let atlas = find_uid(&self.json["defs"]["tilesets"], atlas_uid)
                .ok_or_else(|| Error::Invalid(format!("missing auto-layer tileset {atlas_uid}")))?;
            if atlas["relPath"].is_null()
                && atlas["embedAtlas"].is_null()
                && atlas["pxWid"] == 0
                && atlas["pxHei"] == 0
            {
                updates.push((index, json!([])));
                continue;
            }
            let mut engine = Engine::new(level, layers, li, ld, source, source_def, atlas)?;
            updates.push((index, engine.render(rect)?));
        }
        let level = self.level_mut(key).expect("validated level");
        for (index, tiles) in updates {
            level["layerInstances"][index]["autoLayerTiles"] = tiles;
        }
        Ok(())
    }
}

struct Rule<'a> {
    json: &'a Value,
    pattern: Vec<i32>,
    rects: Vec<Vec<i32>>,
    size: i32,
    uid: i32,
    flip_x: bool,
    flip_y: bool,
}
impl<'a> Rule<'a> {
    fn new(v: &'a Value) -> Result<Self> {
        let size = int(v, "size", 3);
        let pattern = arr(&v["pattern"])
            .iter()
            .map(|x| x.as_i64().unwrap_or(0) as i32)
            .collect::<Vec<_>>();
        if !(1..=MAX_PATTERN_SIZE).contains(&size)
            || size % 2 == 0
            || pattern.len() != (size * size) as usize
        {
            return Err(Error::Invalid("invalid auto-layer rule pattern".into()));
        }
        if int(v, "xModulo", 1) <= 0 || int(v, "yModulo", 1) <= 0 {
            return Err(Error::Invalid("auto-layer modulo must be positive".into()));
        }
        for (min, max) in [
            ("tileRandomXMin", "tileRandomXMax"),
            ("tileRandomYMin", "tileRandomYMax"),
        ] {
            if int(v, min, 0) > int(v, max, 0) {
                return Err(Error::Invalid(
                    "reversed auto-layer random offset range".into(),
                ));
            }
        }
        let rects = if v["tileIds"].is_array() {
            let ids = arr(&v["tileIds"])
                .iter()
                .map(|x| x.as_i64().unwrap_or(-1) as i32)
                .collect::<Vec<_>>();
            if v["tileMode"] == "Stamp" {
                vec![ids]
            } else {
                ids.into_iter().map(|x| vec![x]).collect()
            }
        } else {
            arr(&v["tileRectsIds"])
                .iter()
                .map(|r| {
                    arr(r)
                        .iter()
                        .map(|x| x.as_i64().unwrap_or(-1) as i32)
                        .collect()
                })
                .collect()
        };
        let sym_x = (0..size).all(|y| {
            (0..size).all(|x| {
                pattern[(x + y * size) as usize] == pattern[(size - 1 - x + y * size) as usize]
            })
        });
        let sym_y = (0..size).all(|y| {
            (0..size).all(|x| {
                pattern[(x + y * size) as usize] == pattern[(x + (size - 1 - y) * size) as usize]
            })
        });
        Ok(Self {
            json: v,
            pattern,
            rects,
            size,
            uid: int(v, "uid", 0),
            flip_x: flag(v, "flipX", false) && !sym_x,
            flip_y: flag(v, "flipY", false) && !sym_y,
        })
    }
}
enum Grid<'a> {
    Csv(&'a [Value]),
    Legacy(BTreeMap<i32, i32>),
}
impl Grid<'_> {
    fn get(&self, id: i32) -> i32 {
        match self {
            Self::Csv(v) => v[id as usize].as_i64().unwrap_or(0) as i32,
            Self::Legacy(v) => *v.get(&id).unwrap_or(&0),
        }
    }
}
struct Engine<'a> {
    li: &'a Value,
    ld: &'a Value,
    atlas: &'a Value,
    rules: Vec<Rule<'a>>,
    grid: Grid<'a>,
    groups: BTreeMap<i32, i32>,
    areas: BTreeMap<i32, BTreeSet<i32>>,
    width: i32,
    height: i32,
    sw: i32,
    sh: i32,
    grid_size: i32,
    atlas_width: i32,
    killed: BTreeSet<(i32, i32)>,
    seed: i32,
}
pub(crate) fn group_applied(group: &Value, level: &Value, li: &Value, ld: &Value) -> bool {
    let active = flag(group, "active", true);
    let required = arr(&group["requiredBiomeValues"]);
    if active
        && !required.is_empty()
        && let Some(field) = arr(&level["fieldInstances"])
            .iter()
            .find(|f| !ld["biomeFieldUid"].is_null() && f["defUid"] == ld["biomeFieldUid"])
    {
        let values = if field["__value"].is_array() {
            arr(&field["__value"]).to_vec()
        } else {
            vec![field["__value"].clone()]
        };
        return match int(group, "biomeRequirementMode", 0) {
            0 => required.iter().any(|v| values.contains(v)),
            1 => required.iter().all(|v| values.contains(v)),
            _ => false,
        };
    }
    (active && !flag(group, "isOptional", false))
        || arr(&li["optionalRules"]).contains(&group["uid"])
}

fn active_rules<'a>(
    level: &'a Value,
    layer: &'a Value,
    definition: &'a Value,
) -> impl DoubleEndedIterator<Item = &'a Value> {
    arr(&definition["autoRuleGroups"])
        .iter()
        .filter(move |group| group_applied(group, level, layer, definition))
        .flat_map(|group| arr(&group["rules"]))
        .filter(|rule| flag(rule, "active", true))
}

impl Project {
    /// Order the loaded cache as LDtk draws it, keeping the tile's cached opacity.
    pub fn reconcile_cached_auto_tiles(&mut self) {
        for key in self.level_keys() {
            let Some(level) = self.level(key) else {
                continue;
            };
            let updates = arr(&level["layerInstances"])
                .iter()
                .enumerate()
                .filter_map(|(index, layer)| {
                    let tiles = layer["autoLayerTiles"].as_array()?;
                    if tiles.is_empty() {
                        return None;
                    }
                    let definition = self.layer_def(layer["layerDefUid"].as_i64()?)?;
                    if definition["type"] == "AutoLayer"
                        && linked_rule_source_uid(definition)
                            .and_then(|uid| self.layer_def(uid))
                            .is_none()
                    {
                        return Some((index, json!([])));
                    }
                    let order: BTreeMap<_, _> = active_rules(level, layer, definition)
                        .rev()
                        .enumerate()
                        .filter_map(|(rank, rule)| Some((rule["uid"].as_i64()?, rank)))
                        .collect();
                    let sort_key = |tile: &Value| {
                        let rule = tile["d"][0].as_i64()?;
                        Some((*order.get(&rule)?, tile["d"][1].as_i64()?))
                    };
                    let already_ordered = tiles
                        .iter()
                        .try_fold((0, i64::MIN), |previous, tile| {
                            sort_key(tile).filter(|key| *key >= previous)
                        })
                        .is_some();
                    if already_ordered
                        || tiles
                            .iter()
                            .any(|tile| !tile["d"][0].is_i64() || !tile["d"][1].is_i64())
                    {
                        return None;
                    }
                    let mut sorted: Vec<_> = tiles
                        .iter()
                        .filter_map(|tile| Some((sort_key(tile)?, tile)))
                        .collect();
                    sorted.sort_by_key(|(key, _)| *key);
                    Some((
                        index,
                        Value::Array(sorted.into_iter().map(|(_, tile)| tile.clone()).collect()),
                    ))
                })
                .collect::<Vec<_>>();
            for (index, tiles) in updates {
                self.layer_mut(key, index).expect("enumerated layer")["autoLayerTiles"] = tiles;
            }
        }
    }
}

impl<'a> Engine<'a> {
    fn new(
        level: &'a Value,
        layers: &[Value],
        li: &'a Value,
        ld: &'a Value,
        source: &'a Value,
        source_def: &Value,
        atlas: &'a Value,
    ) -> Result<Self> {
        let width = int(li, "__cWid", 0);
        let height = int(li, "__cHei", 0);
        let sw = int(source, "__cWid", 0);
        let sh = int(source, "__cHei", 0);
        let grid_size = int(ld, "gridSize", int(li, "__gridSize", 16));
        let atlas_size = int(atlas, "tileGridSize", 0);
        let spacing = int(atlas, "spacing", 0);
        let padding = int(atlas, "padding", 0);
        if width < 0
            || height < 0
            || sw < 0
            || sh < 0
            || grid_size <= 0
            || atlas_size <= 0
            || spacing < 0
        {
            return Err(Error::Invalid("invalid auto-layer dimensions".into()));
        }
        let atlas_width = ((int(atlas, "pxWid", 0) - 2 * padding) as f64
            / (atlas_size + spacing) as f64)
            .ceil() as i32;
        if atlas_width <= 0 {
            return Err(Error::Invalid("empty auto-layer tileset".into()));
        }
        let cell_count = (sw as usize)
            .checked_mul(sh as usize)
            .filter(|n| *n <= 100_000_000)
            .ok_or_else(|| Error::Invalid("auto-layer dimensions overflow".into()))?;
        let grid = if let Some(csv) = source["intGridCsv"].as_array() {
            if csv.len() != cell_count {
                return Err(Error::Invalid(
                    "IntGrid CSV length differs from layer dimensions".into(),
                ));
            }
            Grid::Csv(csv)
        } else {
            Grid::Legacy(
                arr(&source["intGrid"])
                    .iter()
                    .map(|c| (int(c, "coordId", -1), int(c, "v", 0) + 1))
                    .collect(),
            )
        };
        let groups = arr(&source_def["intGridValues"])
            .iter()
            .enumerate()
            .map(|(index, v)| (int(v, "value", index as i32 + 1), int(v, "groupUid", 0)))
            .collect::<BTreeMap<_, _>>();
        let areas = BTreeMap::new();
        let rules = active_rules(level, li, ld)
            .map(Rule::new)
            .collect::<Result<Vec<_>>>()?;
        let mut killed = BTreeSet::new();
        for blocker in layers.iter().filter(|l| {
            l["layerDefUid"]
                .as_i64()
                .is_some_and(|uid| blocks_auto_layer(ld, uid))
        }) {
            let blocker_grid = int(blocker, "__gridSize", grid_size).max(1);
            if blocker["__type"] == "IntGrid" {
                let width = int(blocker, "__cWid", 0);
                if width > 0 {
                    for (index, value) in arr(&blocker["intGridCsv"]).iter().enumerate() {
                        if value.as_i64().unwrap_or(0) != 0 {
                            killed.insert((index as i32 % width, index as i32 / width));
                        }
                    }
                }
            } else if blocker["__type"] == "Tiles" || blocker["__type"].is_null() {
                for tile in arr(&blocker["gridTiles"]) {
                    let px = arr(&tile["px"]);
                    if px.len() == 2 {
                        killed.insert((
                            px[0].as_i64().unwrap_or(0) as i32 / blocker_grid,
                            px[1].as_i64().unwrap_or(0) as i32 / blocker_grid,
                        ));
                    }
                }
            }
        }
        Ok(Self {
            li,
            ld,
            atlas,
            rules,
            grid,
            groups,
            areas,
            width,
            height,
            sw,
            sh,
            grid_size,
            atlas_width,
            killed,
            seed: int(li, "seed", 0),
        })
    }
    fn relevant(&self, r: &Rule, x: i32, y: i32) -> bool {
        for &v in &r.pattern {
            if v > 0 && v != 1000001 {
                let Some(areas) = self.areas.get(&v) else {
                    return false;
                };
                let rad = if r.size == 1 { 0 } else { r.size / 2 };
                if ![
                    (x - rad, y - rad),
                    (x + rad, y - rad),
                    (x + rad, y + rad),
                    (x - rad, y + rad),
                ]
                .iter()
                .any(|(a, b)| areas.contains(&(a / 10 + (b / 10) * 10000)))
                {
                    return false;
                }
            }
        }
        true
    }
    fn matches(&self, r: &Rule, x: i32, y: i32, flips: i32) -> bool {
        for py in 0..r.size {
            for px in 0..r.size {
                let p = r.pattern[(px + py * r.size) as usize];
                if p == 0 {
                    continue;
                }
                let sx = x + (px - r.size / 2) * if flips & 1 == 0 { 1 } else { -1 };
                let sy = y + (py - r.size / 2) * if flips & 2 == 0 { 1 } else { -1 };
                let v = if sx >= 0 && sy >= 0 && sx < self.sw && sy < self.sh {
                    self.grid.get(sx + sy * self.sw)
                } else {
                    match r.json["outOfBoundsValue"].as_i64() {
                        Some(v) => v as i32,
                        None => return false,
                    }
                };
                let equal = if p.abs() == 1000001 {
                    v != 0
                } else if p.abs() > 999 {
                    self.groups
                        .get(&v)
                        .is_some_and(|g| *g == p.abs() / 1000 - 1)
                } else {
                    v == p.abs()
                };
                if (p > 0) != equal {
                    return false;
                }
            }
        }
        true
    }
    fn render(&mut self, area: Option<[i32; 4]>) -> Result<Value> {
        let area = area.filter(|_| self.li["autoLayerTiles"].is_array());
        // LDtk expands incremental updates by its maximum supported pattern radius.
        let padding = MAX_PATTERN_SIZE / 2;
        let [left, top, right, bottom] =
            area.map_or([0, 0, self.width, self.height], |[x, y, w, h]| {
                [
                    x.saturating_sub(padding).max(0),
                    y.saturating_sub(padding).max(0),
                    x.saturating_add(w).saturating_add(padding).min(self.width),
                    y.saturating_add(h).saturating_add(padding).min(self.height),
                ]
            });
        // Relevance counts need only the 10-cell source buckets touched by rule
        // footprints. A brush update does not scan or copy the whole IntGrid.
        let radius = self.rules.iter().map(|r| r.size / 2).max().unwrap_or(0);
        let ax = (left.saturating_sub(radius) / 10 * 10).max(0);
        let ay = (top.saturating_sub(radius) / 10 * 10).max(0);
        let bx = ((right.saturating_add(radius) / 10 + 1) * 10).min(self.sw);
        let by = ((bottom.saturating_add(radius) / 10 + 1) * 10).min(self.sh);
        for y in ay..by {
            for x in ax..bx {
                let v = self.grid.get(x + y * self.sw);
                if v != 0 {
                    let bucket = x / 10 + (y / 10) * 10000;
                    self.areas.entry(v).or_default().insert(bucket);
                    if let Some(g) = self.groups.get(&v) {
                        self.areas.entry((g + 1) * 1000).or_default().insert(bucket);
                    }
                }
            }
        }
        let mut preserved: BTreeMap<i32, BTreeMap<i32, Vec<Value>>> = BTreeMap::new();
        if area.is_some() {
            for tile in arr(&self.li["autoLayerTiles"]) {
                let uid = tile["d"][0].as_i64().unwrap_or(-1) as i32;
                let id = tile["d"][1].as_i64().unwrap_or(-1) as i32;
                let x = id % self.width.max(1);
                let y = id / self.width.max(1);
                if x < left || x >= right || y < top || y >= bottom {
                    preserved
                        .entry(uid)
                        .or_default()
                        .entry(id)
                        .or_default()
                        .push(tile.clone());
                }
            }
        }
        let mut output = Vec::with_capacity(self.rules.len());
        let mut locked = BTreeSet::new();
        for r in &self.rules {
            let mut cells = preserved.remove(&r.uid).unwrap_or_default();
            let j = r.json;
            let xm = int(j, "xModulo", 1);
            let ym = int(j, "yModulo", 1);
            let checker = match j["checker"].as_str().unwrap_or("None") {
                _ if xm == 1 && ym == 1 => "None",
                "Horizontal" if xm == 1 => "Vertical",
                "Vertical" if ym == 1 => "Horizontal",
                mode => mode,
            };
            let chance = float(j, "chance", 1.);
            for y in top..bottom {
                for x in left..right {
                    let id = x + y * self.width;
                    if locked.contains(&id)
                        || self.killed.contains(&(x, y))
                        || r.rects.is_empty()
                        || !self.relevant(r, x, y)
                    {
                        continue;
                    }
                    if (checker != "Vertical" && (y - int(j, "yOffset", 0)) % ym != 0)
                        || (checker == "Vertical" && (y + (x / xm) % 2) % ym != 0)
                        || (checker != "Horizontal" && (x - int(j, "xOffset", 0)) % xm != 0)
                        || (checker == "Horizontal" && (x + (y / ym) % 2) % xm != 0)
                    {
                        continue;
                    }
                    if chance <= 0.
                        || (chance < 1.
                            && noise::random(self.seed as f64 + r.uid as f64, x, y, 100) as f64
                                >= chance * 100.)
                    {
                        continue;
                    }
                    if flag(j, "perlinActive", false)
                        && auto_rule_perlin_value(self.seed, [x, y], j) < 0.
                    {
                        continue;
                    }
                    let mut tiles = Vec::new();
                    for flips in 0..4 {
                        if (flips & 1 != 0 && !r.flip_x)
                            || (flips & 2 != 0 && !r.flip_y)
                            || !self.matches(r, x, y, flips)
                        {
                            continue;
                        }
                        self.add_tiles(r, x, y, flips, &mut tiles)?;
                        if flag(j, "breakOnMatch", false) {
                            break;
                        }
                    }
                    if !tiles.is_empty() {
                        let no_offset = [
                            "tileXOffset",
                            "tileYOffset",
                            "tileRandomXMin",
                            "tileRandomXMax",
                            "tileRandomYMin",
                            "tileRandomYMax",
                        ]
                        .iter()
                        .all(|k| int(j, k, 0) == 0);
                        let opaque = self.atlas["cachedPixelData"]["opaqueTiles"]
                            .as_str()
                            .unwrap_or("")
                            .as_bytes();
                        if flag(j, "breakOnMatch", false)
                            || (no_offset
                                && float(j, "alpha", 1.) >= 1.
                                && tiles.iter().any(|t| {
                                    opaque.get(t["t"].as_u64().unwrap_or(u64::MAX) as usize)
                                        == Some(&b'1')
                                }))
                        {
                            locked.insert(id);
                        }
                        cells.insert(id, tiles);
                    }
                }
            }
            output.push(cells);
        }
        Ok(Value::Array(
            output
                .into_iter()
                .rev()
                .flat_map(|cells| cells.into_values().flatten())
                .collect(),
        ))
    }
    fn add_tiles(&self, r: &Rule, x: i32, y: i32, flips: i32, out: &mut Vec<Value>) -> Result<()> {
        let j = r.json;
        let seed = self.seed as f64 + r.uid as f64;
        let rect =
            &r.rects[noise::random(seed + flips as f64, x, y, r.rects.len() as i32) as usize];
        let left = rect.iter().map(|t| t % self.atlas_width).min().unwrap_or(0);
        let right = rect.iter().map(|t| t % self.atlas_width).max().unwrap_or(0);
        let top = rect.iter().map(|t| t / self.atlas_width).min().unwrap_or(0);
        let bottom = rect.iter().map(|t| t / self.atlas_width).max().unwrap_or(0);
        let dx = if flips & 1 == 0 { 1 } else { -1 };
        let dy = if flips & 2 == 0 { 1 } else { -1 };
        let rx = int(j, "tileRandomXMin", 0);
        let ry = int(j, "tileRandomYMin", 0);
        let ox = (int(j, "tileXOffset", 0)
            + noise::random(
                seed + flips as f64,
                x,
                y,
                int(j, "tileRandomXMax", 0) - rx + 1,
            )
            + rx)
            * dx;
        let oy = (int(j, "tileYOffset", 0)
            + noise::random(seed + 1., x, y, int(j, "tileRandomYMax", 0) - ry + 1)
            + ry)
            * dy;
        let tile_size = int(self.atlas, "tileGridSize", 16);
        let padding = int(self.atlas, "padding", 0);
        let spacing = int(self.atlas, "spacing", 0);
        for &tid in rect {
            if tid < 0 {
                return Err(Error::Invalid("negative auto-layer tile ID".into()));
            }
            let tx = tid % self.atlas_width;
            let ty = tid / self.atlas_width;
            let sx = if j["tileMode"] == "Stamp" {
                ((tx - left) as f64 - float(j, "pivotX", 0.) * (right - left) as f64
                    + float(self.ld, "tilePivotX", 0.))
                    * self.grid_size as f64
            } else {
                0.
            };
            let sy = if j["tileMode"] == "Stamp" {
                ((ty - top) as f64 - float(j, "pivotY", 0.) * (bottom - top) as f64
                    + float(self.ld, "tilePivotY", 0.))
                    * self.grid_size as f64
            } else {
                0.
            };
            out.push(json!({"px":[x*self.grid_size+sx as i32*dx+ox,y*self.grid_size+sy as i32*dy+oy],"src":[padding+tx*(tile_size+spacing),padding+ty*(tile_size+spacing)],"f":flips,"t":tid,"d":[r.uid,x+y*self.width],"a":number(float(j,"alpha",1.))}));
        }
        Ok(())
    }
}
