//! The signed text envelope used by the original editor's system clipboard.
use crate::autotile::{tidy_auto_rule_group, tidy_layer_auto_rules, tidy_rule};
use crate::workflow::{alloc_uid, alloc_uids, assign_rule_uids};
use crate::{Error, Project, Result};
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Rule,
    RuleGroup,
    LayerDef,
    EntityDef,
    EnumDef,
    TilesetDef,
    FieldDef,
}
impl ClipboardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "CRule",
            Self::RuleGroup => "CRuleGroup",
            Self::LayerDef => "CLayerDef",
            Self::EntityDef => "CEntityDef",
            Self::EnumDef => "CEnumDef",
            Self::TilesetDef => "CTilesetDef",
            Self::FieldDef => "CFieldDef",
        }
    }
    fn definition_list(self) -> Option<&'static str> {
        match self {
            Self::LayerDef => Some("layers"),
            Self::EntityDef => Some("entities"),
            Self::EnumDef => Some("enums"),
            Self::TilesetDef => Some("tilesets"),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardData {
    pub kind: ClipboardKind,
    pub value: Value,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
impl ClipboardData {
    pub fn encode(&self, version: &str) -> Result<String> {
        if version.is_empty() || version.chars().any(char::is_whitespace) || !self.value.is_object()
        {
            return Err(invalid("invalid clipboard version or object"));
        }
        Ok(format!(
            "LDtk {version} clipboard\n----\n{}\n----\n{}",
            self.kind.as_str(),
            self.value
        ))
    }
    pub fn decode(text: &str) -> Result<Self> {
        let raw = text.replace('\r', "");
        let parts = raw.split("\n----\n").collect::<Vec<_>>();
        if parts.len() != 3 || !parts[0].starts_with("LDtk ") || !parts[0].ends_with(" clipboard") {
            return Err(invalid("clipboard does not contain signed LDtk data"));
        }
        let version = &parts[0][5..parts[0].len() - 10];
        if version.is_empty() || version.chars().any(char::is_whitespace) {
            return Err(invalid("invalid clipboard signature"));
        }
        let kind = match parts[1] {
            "CRule" => ClipboardKind::Rule,
            "CRuleGroup" => ClipboardKind::RuleGroup,
            "CLayerDef" => ClipboardKind::LayerDef,
            "CEntityDef" => ClipboardKind::EntityDef,
            "CEnumDef" => ClipboardKind::EnumDef,
            "CTilesetDef" => ClipboardKind::TilesetDef,
            "CFieldDef" => ClipboardKind::FieldDef,
            _ => return Err(invalid("unknown LDtk clipboard type")),
        };
        let value = serde_json::from_str::<Value>(parts[2])
            .map_err(|_| invalid("invalid clipboard JSON"))?;
        if !value.is_object() {
            return Err(invalid("clipboard data must be an object"));
        }
        Ok(Self { kind, value })
    }
}
fn unique_name(base: &str, list: &[Value], style: &str) -> String {
    let occupied = list
        .iter()
        .filter_map(|d| d["identifier"].as_str())
        .map(str::to_owned)
        .collect();
    crate::naming::unique_name(base, style, &occupied)
}
fn definition<'a>(p: &'a Project, kind: &str, uid: i64) -> Result<&'a Value> {
    p.json["defs"][kind]
        .as_array()
        .and_then(|a| a.iter().find(|d| d["uid"] == uid))
        .ok_or_else(|| invalid("clipboard definition not found"))
}
fn field_list(p: &Project, owner: Option<i64>) -> Result<&Vec<Value>> {
    let list = if let Some(uid) = owner {
        &definition(p, "entities", uid)?["fieldDefs"]
    } else {
        &p.json["defs"]["levelFields"]
    };
    list.as_array().ok_or_else(|| invalid("field list missing"))
}
fn field_list_mut(p: &mut Project, owner: Option<i64>) -> Result<&mut Vec<Value>> {
    let list = if let Some(uid) = owner {
        &mut p.json["defs"]["entities"]
            .as_array_mut()
            .and_then(|a| a.iter_mut().find(|d| d["uid"] == uid))
            .ok_or_else(|| invalid("field owner missing"))?["fieldDefs"]
    } else {
        &mut p.json["defs"]["levelFields"]
    };
    list.as_array_mut()
        .ok_or_else(|| invalid("field list missing"))
}
fn validate_dependencies(p: &Project, value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "layerUidsPreventingAutoTilingHere" {
                    let blockers = value
                        .as_array()
                        .ok_or_else(|| invalid("invalid clipboard blocker layers"))?;
                    for blocker in blockers {
                        let uid = blocker
                            .as_i64()
                            .ok_or_else(|| invalid("invalid clipboard blocker UID"))?;
                        let layer = definition(p, "layers", uid)?;
                        if !matches!(layer["type"].as_str(), Some("Tiles" | "IntGrid")) {
                            return Err(invalid(
                                "clipboard blocker must be a Tiles or IntGrid layer",
                            ));
                        }
                    }
                }
                let group = match key.as_str() {
                    "tilesetDefUid" | "tilesetId" | "tilesetUid" | "iconTilesetUid" => {
                        Some("tilesets")
                    }
                    "autoSourceLayerDefUid" => Some("layers"),
                    "biomeFieldUid" => Some("levelFields"),
                    "allowedRefsEntityUid" => Some("entities"),
                    "enumTagsSourceEnumUid" | "tagsSourceEnumUid" => Some("enum"),
                    _ => None,
                };
                if let Some(group) = group
                    && let Some(uid) = value.as_i64()
                {
                    let exists = if group == "enum" {
                        definition(p, "enums", uid).is_ok()
                            || definition(p, "externalEnums", uid).is_ok()
                    } else {
                        definition(p, group, uid).is_ok()
                    };
                    if !exists {
                        return Err(invalid(&format!(
                            "clipboard dependency {key}={uid} is absent from this project"
                        )));
                    }
                }
                if key == "type"
                    && let Some(kind) = value.as_str()
                    && let Some(raw) = kind
                        .strip_prefix("F_Enum(")
                        .and_then(|s| s.strip_suffix(')'))
                {
                    let uid = raw
                        .parse::<i64>()
                        .map_err(|_| invalid("invalid enum field type in clipboard"))?;
                    if definition(p, "enums", uid).is_err()
                        && definition(p, "externalEnums", uid).is_err()
                    {
                        return Err(invalid(
                            "clipboard enum definition is absent from this project",
                        ));
                    }
                }
                validate_dependencies(p, value)?;
            }
        }
        Value::Array(a) => {
            for v in a {
                validate_dependencies(p, v)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn move_after(list: &mut Vec<Value>, uid: i64, after: Option<i64>) -> Result<()> {
    let index = list
        .iter()
        .position(|d| d["uid"] == uid)
        .ok_or_else(|| invalid("new clipboard object missing"))?;
    let item = list.remove(index);
    let target = if let Some(after) = after {
        list.iter()
            .position(|d| d["uid"] == after)
            .map(|i| i + 1)
            .ok_or_else(|| invalid("clipboard insertion point missing"))?
    } else {
        list.len()
    };
    list.insert(target, item);
    Ok(())
}
fn groups_mut(p: &mut Project, layer: i64) -> Result<&mut Vec<Value>> {
    let def = p.json["defs"]["layers"]
        .as_array_mut()
        .and_then(|a| a.iter_mut().find(|d| d["uid"] == layer))
        .ok_or_else(|| invalid("rule layer missing"))?;
    if def["type"] != "IntGrid" && def["type"] != "AutoLayer" {
        return Err(invalid("rules require an IntGrid or AutoLayer"));
    }
    def["autoRuleGroups"]
        .as_array_mut()
        .ok_or_else(|| invalid("rule groups missing"))
}
fn clipboard_rule(uid: i64, raw: &Value) -> Result<Value> {
    let mut patch = raw.clone();
    let obj = patch
        .as_object_mut()
        .ok_or_else(|| invalid("clipboard rule must be an object"))?;
    obj.remove("uid");
    if let Some(ids) = obj.remove("tileIds")
        && !obj.contains_key("tileRectsIds")
    {
        obj.insert(
            "tileRectsIds".into(),
            Value::Array(
                ids.as_array()
                    .ok_or_else(|| invalid("legacy tile IDs must be an array"))?
                    .iter()
                    .map(|v| json!([v]))
                    .collect(),
            ),
        );
    }
    crate::autotile::clipboard_rule(uid, &patch)
}
impl Project {
    pub fn copy_definition(&self, kind: &str, uid: i64) -> Result<ClipboardData> {
        let clip = match kind {
            "layers" => ClipboardKind::LayerDef,
            "entities" => ClipboardKind::EntityDef,
            "enums" | "externalEnums" => ClipboardKind::EnumDef,
            "tilesets" => ClipboardKind::TilesetDef,
            _ => return Err(invalid("unsupported definition clipboard kind")),
        };
        let mut value = definition(self, kind, uid)?.clone();
        if clip == ClipboardKind::LayerDef {
            let source = self.auto_rule_source_values(&value);
            tidy_layer_auto_rules(&mut value, Some(&source));
        }
        Ok(ClipboardData { kind: clip, value })
    }
    pub fn paste_definition(&mut self, data: &ClipboardData, after: Option<i64>) -> Result<i64> {
        self.edit_document(|p| {
            let kind = data
                .kind
                .definition_list()
                .ok_or_else(|| invalid("clipboard does not contain a definition"))?;
            validate_dependencies(p, &data.value)?;
            let defs = p.json["defs"][kind]
                .as_array()
                .ok_or_else(|| invalid("definition list missing"))?;
            let mut names = defs.clone();
            if kind == "enums" {
                names.extend(
                    p.json["defs"]["externalEnums"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
            let name = unique_name(
                data.value["identifier"].as_str().unwrap_or("Copy"),
                &names,
                p.json["identifierStyle"].as_str().unwrap_or("Capitalize"),
            );
            let mut copy = data.value.clone();
            let uid = if kind == "layers" {
                p.add_layer(
                    &name,
                    copy["type"]
                        .as_str()
                        .ok_or_else(|| invalid("clipboard layer type missing"))?,
                    copy["gridSize"]
                        .as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or_else(|| invalid("clipboard layer grid missing"))?,
                )?
            } else {
                alloc_uid(p)?
            };
            copy["uid"] = json!(uid);
            copy["identifier"] = json!(name);
            if kind == "entities" {
                let fields = copy["fieldDefs"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("clipboard entity fields missing"))?;
                let uids = alloc_uids(p, fields.len())?;
                for (field, uid) in fields.iter_mut().zip(uids) {
                    field["uid"] = json!(uid);
                    p.make_field_instance(field)?;
                }
            }
            if kind == "layers" {
                assign_rule_uids(p, &mut copy)?;
            }
            let defs = p.json["defs"][kind].as_array_mut().expect("validated list");
            if kind == "layers" {
                *defs
                    .iter_mut()
                    .find(|d| d["uid"] == uid)
                    .expect("new layer") = copy;
            } else {
                defs.push(copy);
            }
            move_after(defs, uid, after)?;
            if kind == "layers" {
                let index = p.json["defs"][kind]
                    .as_array()
                    .expect("list")
                    .iter()
                    .position(|d| d["uid"] == uid)
                    .expect("new layer");
                p.reorder_layer(uid, index)?;
            }
            p.refresh_auto_and_derived()?;
            Ok(uid)
        })
    }
    pub fn copy_field_definition(&self, uid: i64) -> Result<ClipboardData> {
        let fields = self.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|d| d["fieldDefs"].as_array().into_iter().flatten())
            .chain(
                self.json["defs"]["levelFields"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            );
        let value = fields
            .into_iter()
            .find(|f| f["uid"] == uid)
            .ok_or_else(|| invalid("field definition not found"))?
            .clone();
        Ok(ClipboardData {
            kind: ClipboardKind::FieldDef,
            value,
        })
    }
    pub fn paste_field_definition(
        &mut self,
        owner: Option<i64>,
        data: &ClipboardData,
        after: Option<i64>,
    ) -> Result<i64> {
        self.edit_document(|p| {
            if data.kind != ClipboardKind::FieldDef {
                return Err(invalid("clipboard does not contain a field"));
            }
            validate_dependencies(p, &data.value)?;
            let name = unique_name(
                data.value["identifier"].as_str().unwrap_or("Field"),
                field_list(p, owner)?,
                "Free",
            );
            let public = data.value["__type"]
                .as_str()
                .ok_or_else(|| invalid("clipboard field type missing"))?;
            let kind = public.trim_start_matches("Array<").trim_end_matches('>');
            let array = data.value["isArray"]
                .as_bool()
                .ok_or_else(|| invalid("clipboard array flag missing"))?;
            let uid = p.add_field_definition(owner, &name, kind, array)?;
            let mut copy = data.value.clone();
            copy["uid"] = json!(uid);
            copy["identifier"] = json!(name);
            p.make_field_instance(&copy)?;
            let list = field_list_mut(p, owner)?;
            *list
                .iter_mut()
                .find(|d| d["uid"] == uid)
                .expect("new field") = copy;
            move_after(list, uid, after)?;
            p.refresh_derived_data()?;
            Ok(uid)
        })
    }
    /// Source "Duplicate and remap": map absolute IntGrid IDs while preserving exclusions,
    /// then transpose every output rectangle without wrapping across atlas edges.
    pub fn duplicate_rule_group_remapped(
        &mut self,
        layer: i64,
        group: i64,
        name: &str,
        remaps: &std::collections::BTreeMap<i64, i64>,
        offset: [i32; 2],
    ) -> Result<i64> {
        let data = self.remap_rule_group_clipboard(layer, group, name, remaps, offset)?;
        self.paste_auto_rule(layer, group, &data, Some(group))
    }
    pub fn remap_rule_group_clipboard(
        &self,
        layer: i64,
        group: i64,
        name: &str,
        remaps: &std::collections::BTreeMap<i64, i64>,
        offset: [i32; 2],
    ) -> Result<ClipboardData> {
        if name.trim().is_empty() {
            return Err(invalid("group name must not be empty"));
        }
        let def = definition(self, "layers", layer)?;
        let atlas = def["tilesetDefUid"]
            .as_i64()
            .and_then(|uid| self.tileset(uid))
            .ok_or_else(|| invalid("group remapping requires a tileset"))?;
        let grid = atlas["tileGridSize"]
            .as_i64()
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("invalid tileset grid"))?;
        let padding = atlas["padding"].as_i64().unwrap_or(0);
        let spacing = atlas["spacing"].as_i64().unwrap_or(0);
        if grid + spacing <= 0 {
            return Err(invalid("invalid tileset spacing"));
        }
        let cols =
            (atlas["pxWid"].as_i64().unwrap_or(0) - 2 * padding + spacing) / (grid + spacing);
        let rows =
            (atlas["pxHei"].as_i64().unwrap_or(0) - 2 * padding + spacing) / (grid + spacing);
        if cols <= 0 || rows <= 0 {
            return Err(invalid("invalid tileset dimensions"));
        }
        if remaps.iter().any(|(old, new)| {
            *old <= 0 || *new < 0 || *old == 1000001 || *new > i64::from(i32::MAX)
        }) {
            return Err(invalid("invalid IntGrid ID remapping"));
        }
        let mut data = self.copy_auto_rule(layer, group, None)?;
        data.value["name"] = json!(name);
        for rule in data.value["rules"]
            .as_array_mut()
            .ok_or_else(|| invalid("rule list missing"))?
        {
            for v in rule["pattern"]
                .as_array_mut()
                .ok_or_else(|| invalid("rule pattern missing"))?
            {
                let old = v
                    .as_i64()
                    .ok_or_else(|| invalid("pattern must contain integers"))?;
                if let Some(new) = remaps.get(
                    &old.checked_abs()
                        .ok_or_else(|| invalid("pattern ID exceeds integer range"))?,
                ) {
                    *v = json!(if old < 0 { -*new } else { *new });
                }
            }
            if let Some(old) = rule["outOfBoundsValue"].as_i64()
                && let Some(new) = remaps.get(&old)
            {
                rule["outOfBoundsValue"] = json!(new);
            }
            for rect in rule["tileRectsIds"].as_array_mut().into_iter().flatten() {
                for tile in rect
                    .as_array_mut()
                    .ok_or_else(|| invalid("rule tile rectangle missing"))?
                {
                    let old = tile
                        .as_i64()
                        .filter(|n| *n >= 0 && *n < cols * rows)
                        .ok_or_else(|| invalid("tile outside source atlas"))?;
                    let x = old % cols + i64::from(offset[0]);
                    let y = old / cols + i64::from(offset[1]);
                    if !(0..cols).contains(&x) || !(0..rows).contains(&y) {
                        return Err(invalid("transposed tiles extend beyond the atlas"));
                    }
                    *tile = json!(x + y * cols);
                }
            }
        }
        Ok(data)
    }
    pub fn copy_auto_rule(
        &self,
        layer: i64,
        group: i64,
        rule: Option<i64>,
    ) -> Result<ClipboardData> {
        let layer = definition(self, "layers", layer)?;
        let group = layer["autoRuleGroups"]
            .as_array()
            .and_then(|a| a.iter().find(|g| g["uid"] == group))
            .ok_or_else(|| invalid("rule group missing"))?;
        let source = self.auto_rule_source_values(layer);
        let (kind, value) = if let Some(uid) = rule {
            let mut value = group["rules"]
                .as_array()
                .and_then(|a| a.iter().find(|r| r["uid"] == uid))
                .ok_or_else(|| invalid("rule missing"))?
                .clone();
            tidy_rule(&mut value, Some(&source));
            (ClipboardKind::Rule, value)
        } else {
            let mut value = group.clone();
            tidy_auto_rule_group(&mut value, Some(&source));
            (ClipboardKind::RuleGroup, value)
        };
        Ok(ClipboardData { kind, value })
    }
    pub fn paste_auto_rule(
        &mut self,
        layer: i64,
        group: i64,
        data: &ClipboardData,
        after: Option<i64>,
    ) -> Result<i64> {
        if data.kind == ClipboardKind::RuleGroup {
            return self.paste_auto_rule_group(layer, data, after);
        }
        self.edit_document(|p| {
            validate_dependencies(p, &data.value)?;
            if data.kind != ClipboardKind::Rule {
                return Err(invalid("clipboard does not contain a rule or group"));
            }
            let uid = alloc_uid(p)?;
            let copy = clipboard_rule(uid, &data.value)?;
            let rules = groups_mut(p, layer)?
                .iter_mut()
                .find(|g| g["uid"] == group)
                .ok_or_else(|| invalid("rule group missing"))?["rules"]
                .as_array_mut()
                .ok_or_else(|| invalid("rule list missing"))?;
            rules.push(copy);
            move_after(rules, uid, after)?;
            p.refresh_auto_and_derived()?;
            Ok(uid)
        })
    }

    pub fn paste_auto_rule_group(
        &mut self,
        layer: i64,
        data: &ClipboardData,
        after: Option<i64>,
    ) -> Result<i64> {
        if data.kind != ClipboardKind::RuleGroup {
            return Err(invalid("clipboard does not contain a rule group"));
        }
        self.edit_document(|p| {
            validate_dependencies(p, &data.value)?;
            let name = data.value["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| invalid("clipboard group name missing"))?;
            let rules = data.value["rules"]
                .as_array()
                .ok_or_else(|| invalid("clipboard group rules missing"))?;
            let mut uids = alloc_uids(p, 1 + rules.len())?;
            let uid = uids.next().expect("reserved group UID");
            let generated = uids
                .zip(rules)
                .map(|(uid, rule)| clipboard_rule(uid, rule))
                .collect::<Result<Vec<_>>>()?;
            let mut copy = json!({
                "uid": uid, "name": name, "color": null, "icon": null, "active": true,
                "isOptional": false, "usesWizard": false, "requiredBiomeValues": [],
                "biomeRequirementMode": 0, "rules": []
            });
            copy.as_object_mut().expect("object").extend(
                data.value
                    .as_object()
                    .ok_or_else(|| invalid("clipboard group must be an object"))?
                    .clone(),
            );
            copy["uid"] = json!(uid);
            copy["rules"] = json!(generated);
            let groups = groups_mut(p, layer)?;
            groups.push(copy);
            move_after(groups, uid, after)?;
            p.refresh_auto_and_derived()?;
            Ok(uid)
        })
    }
}
