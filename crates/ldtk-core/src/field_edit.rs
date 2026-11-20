//! LDtk public field values and the original ValueWrapper editor representation.
use crate::workflow::{alloc_uid, invalid, valid_identifier, visit_fields_mut};
use crate::{Error, LevelKey, Project, Result};
use serde_json::{Value, json};

/// One source editor array action, preserving every untouched ValueWrapper.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldArrayEdit {
    Set { index: usize, value: Value },
    Insert { index: usize },
    InsertValue { index: usize, value: Value },
    Remove { index: usize },
    Move { from: usize, to: usize },
    Reset { index: usize },
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}
fn unescape(s: &str) -> String {
    s.replace("\\\\", "\\").replace("\\n", "\n")
}
fn base(d: &Value) -> &str {
    let s = d["__type"].as_str().unwrap_or("");
    s.strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(s)
}
fn array(d: &Value) -> bool {
    d["isArray"].as_bool().unwrap_or_else(|| {
        d["__type"]
            .as_str()
            .is_some_and(|s| s.starts_with("Array<"))
    })
}
fn field_defs(p: &Project) -> Vec<Value> {
    let mut all = p.json["defs"]["levelFields"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(entities) = p.json["defs"]["entities"].as_array() {
        for e in entities {
            if let Some(fields) = e["fieldDefs"].as_array() {
                all.extend(fields.iter().cloned());
            }
        }
    }
    all
}
fn field_def(p: &Project, uid: i64) -> Result<Value> {
    field_defs(p)
        .into_iter()
        .find(|d| d["uid"] == uid)
        .ok_or_else(|| invalid("field definition not found"))
}
fn field_def_mut(p: &mut Project, uid: i64) -> Option<&mut Value> {
    let defs = p.json["defs"].as_object_mut()?;
    let entity_owner = defs
        .get("entities")
        .and_then(Value::as_array)
        .and_then(|entities| {
            entities.iter().position(|e| {
                e["fieldDefs"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|f| f["uid"] == uid))
            })
        });
    if let Some(i) = entity_owner {
        defs.get_mut("entities")?
            .get_mut(i)?
            .get_mut("fieldDefs")?
            .as_array_mut()?
            .iter_mut()
            .find(|d| d["uid"] == uid)
    } else {
        defs.get_mut("levelFields")?
            .as_array_mut()?
            .iter_mut()
            .find(|d| d["uid"] == uid)
    }
}
fn type_data(p: &Project, kind: &str, is_array: bool) -> Result<(String, Value)> {
    let internal = match kind {
        "Int" => json!("F_Int"),
        "Float" => json!("F_Float"),
        "Bool" => json!("F_Bool"),
        "Color" => json!("F_Color"),
        "String" => json!("F_String"),
        "Multilines" => json!("F_Text"),
        "FilePath" => json!("F_Path"),
        "Point" => json!("F_Point"),
        "Tile" => json!("F_Tile"),
        "EntityRef" => json!("F_EntityRef"),
        _ => {
            let (defs, name) = if let Some(s) = kind.strip_prefix("LocalEnum.") {
                ("enums", s)
            } else if let Some(s) = kind.strip_prefix("ExternEnum.") {
                ("externalEnums", s)
            } else {
                return Err(invalid("unknown field type"));
            };
            let e = p.json["defs"][defs]
                .as_array()
                .and_then(|defs| defs.iter().find(|e| e["identifier"] == name))
                .ok_or_else(|| invalid("enum definition not found"))?;
            json!(format!("F_Enum({})", e["uid"]))
        }
    };
    Ok((
        if is_array {
            format!("Array<{kind}>")
        } else {
            kind.into()
        },
        internal,
    ))
}
fn wrapper_value(v: &Value, depth: usize) -> Value {
    let mut value = v["params"][0].clone();
    if let Some(s) = value.as_str() {
        let mut s = s.to_owned();
        for _ in 0..depth {
            s = unescape(&s);
        }
        value = json!(s);
    }
    value
}
fn wrapper(d: &Value, v: &Value, depth: usize) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let kind = base(d);
    let (id, mut raw) = match kind {
        "Int" => (
            "V_Int",
            json!(
                v.as_i64()
                    .ok_or_else(|| invalid("Int value must be an integer"))?
            ),
        ),
        "Float" => (
            "V_Float",
            json!(
                v.as_f64()
                    .ok_or_else(|| invalid("Float value must be numeric"))?
            ),
        ),
        "Bool" => (
            "V_Bool",
            json!(
                v.as_bool()
                    .ok_or_else(|| invalid("Bool value must be boolean"))?
            ),
        ),
        "Color" => {
            let color = if let Some(s) = v.as_str() {
                i64::from_str_radix(s.strip_prefix('#').unwrap_or(s), 16)
                    .map_err(|_| invalid("invalid color"))?
            } else {
                v.as_i64().ok_or_else(|| invalid("invalid color"))?
            };
            if !(0..=0xFFFFFF).contains(&color) {
                return Err(invalid("color outside RGB range"));
            }
            ("V_Int", json!(color))
        }
        "Point" => {
            let x = v["cx"]
                .as_i64()
                .ok_or_else(|| invalid("Point value requires integer cx,cy"))?;
            let y = v["cy"]
                .as_i64()
                .ok_or_else(|| invalid("Point value requires integer cx,cy"))?;
            ("V_String", json!(format!("{x},{y}")))
        }
        "Tile" => {
            let mut coords = vec![];
            for k in ["x", "y", "w", "h"] {
                coords.push(
                    v[k].as_i64()
                        .ok_or_else(|| invalid("Tile value requires integer x,y,w,h"))?,
                );
            }
            if coords[0] < 0 || coords[1] < 0 || coords[2] <= 0 || coords[3] <= 0 {
                return Err(invalid("invalid Tile rectangle"));
            }
            (
                "V_String",
                json!(
                    coords
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            )
        }
        "EntityRef" => (
            "V_String",
            json!(
                v.as_str()
                    .or_else(|| v["entityIid"].as_str())
                    .ok_or_else(|| invalid("EntityRef requires entity IID"))?
            ),
        ),
        _ => (
            "V_String",
            json!(
                v.as_str()
                    .ok_or_else(|| invalid("field value must be a string"))?
            ),
        ),
    };
    if let Some(s) = raw.as_str() {
        let mut s = s.to_owned();
        for _ in 0..depth {
            s = escape(&s);
        }
        raw = json!(s);
    }
    Ok(json!({"id":id,"params":[raw]}))
}
fn value_from_raw(p: &Project, d: &Value, raw: Value) -> Result<Value> {
    if raw.is_null() {
        return Ok(Value::Null);
    }
    let kind = base(d);
    Ok(match kind {
        "Int" | "Float" => {
            let mut n = raw
                .as_f64()
                .ok_or_else(|| invalid("invalid numeric wrapper"))?;
            if let Some(min) = d["min"].as_f64() {
                n = n.max(if kind == "Int" {
                    (min + 0.5).floor()
                } else {
                    min
                });
            }
            if let Some(max) = d["max"].as_f64() {
                n = n.min(if kind == "Int" {
                    (max + 0.5).floor()
                } else {
                    max
                });
            }
            if kind == "Int" {
                json!(n as i64)
            } else {
                let n = (n * 1000. + 0.5).floor() / 1000.;
                if n.fract() == 0. && n >= i64::MIN as f64 && n < i64::MAX as f64 {
                    json!(n as i64)
                } else {
                    json!(n)
                }
            }
        }
        "Bool" => raw,
        "Color" => json!(format!(
            "#{:06X}",
            raw.as_i64()
                .ok_or_else(|| invalid("invalid Color wrapper"))?
                & 0xFFFFFF
        )),
        "Point" | "Tile" => {
            let parts = raw
                .as_str()
                .ok_or_else(|| invalid("invalid coordinate wrapper"))?
                .split(',')
                .map(str::parse::<i64>)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| invalid("invalid coordinate numbers"))?;
            if kind == "Point" && parts.len() == 2 {
                json!({"cx":parts[0],"cy":parts[1]})
            } else if kind == "Tile" && parts.len() == 4 {
                json!({"tilesetUid":d["tilesetUid"],"x":parts[0],"y":parts[1],"w":parts[2],"h":parts[3]})
            } else {
                return Err(invalid("invalid coordinate count"));
            }
        }
        "EntityRef" => {
            let id = raw
                .as_str()
                .ok_or_else(|| invalid("invalid entity reference wrapper"))?;
            let mut found = None;
            for key in p.level_keys() {
                let level = p.level(key).ok_or_else(|| invalid("level not loaded"))?;
                if let Some(layers) = level["layerInstances"].as_array() {
                    for l in layers {
                        if l["entityInstances"]
                            .as_array()
                            .is_some_and(|es| es.iter().any(|e| e["iid"] == id))
                        {
                            let wiid = key.world.map_or_else(
                                || p.json["dummyWorldIid"].clone(),
                                |w| p.json["worlds"][w]["iid"].clone(),
                            );
                            found = Some(
                                json!({"entityIid":id,"layerIid":l["iid"],"levelIid":level["iid"],"worldIid":wiid}),
                            );
                        }
                    }
                }
            }
            found.unwrap_or_else(
                || json!({"entityIid":id,"layerIid":"?","levelIid":"?","worldIid":"?"}),
            )
        }
        s if s.starts_with("LocalEnum.") || s.starts_with("ExternEnum.") => raw,
        _ => json!(escape(
            raw.as_str()
                .ok_or_else(|| invalid("invalid String wrapper"))?
        )),
    })
}
fn enum_definition<'a>(p: &'a Project, d: &Value) -> Option<&'a Value> {
    let name = base(d).split_once('.').map(|(_, name)| name).unwrap_or("");
    let uid = d["type"]
        .as_str()
        .and_then(|s| s.strip_prefix("F_Enum("))
        .and_then(|s| s.strip_suffix(')'))
        .and_then(|s| s.parse::<i64>().ok());
    ["enums", "externalEnums"].iter().find_map(|group| {
        p.json["defs"][*group]
            .as_array()?
            .iter()
            .find(|e| uid.map_or_else(|| e["identifier"] == name, |uid| e["uid"] == uid))
    })
}
fn enum_field_value<'a>(p: &'a Project, d: &Value, value: &Value) -> Option<&'a Value> {
    let uid = enum_definition(p, d)?["uid"].as_i64()?;
    p.enum_value(uid, value.as_str()?)
}
fn enum_has_value(p: &Project, d: &Value, value: &Value) -> bool {
    enum_field_value(p, d, value).is_some()
}
impl Project {
    pub fn cleanup_identifier(&self, input: &str) -> String {
        crate::naming::cleanup(
            input,
            self.json["identifierStyle"]
                .as_str()
                .unwrap_or("Capitalize"),
        )
    }
    /// Original EnumDef.getValue: local enums normalize a lookup, external enums compare verbatim.
    /// Stored legacy wrappers keep their original strings when this lookup succeeds.
    pub fn enum_value(&self, uid: i64, value: &str) -> Option<&Value> {
        let definition = ["enums", "externalEnums"].iter().find_map(|kind| {
            self.json["defs"][*kind]
                .as_array()?
                .iter()
                .find(|e| e["uid"] == uid)
        })?;
        let value = if definition["externalRelPath"].is_string() {
            value.to_owned()
        } else {
            self.cleanup_identifier(value)
        };
        definition["values"]
            .as_array()?
            .iter()
            .find(|v| v["id"] == value)
    }
}
fn default_value(p: &Project, d: &Value) -> Result<Value> {
    let kind = base(d);
    if ["Point", "FilePath", "EntityRef"].contains(&kind) {
        return Ok(Value::Null);
    }
    let raw = if !d["defaultOverride"].is_null() {
        wrapper_value(&d["defaultOverride"], 1)
    } else if d["canBeNull"] == true {
        Value::Null
    } else {
        match kind {
            "Int" | "Float" | "Color" => json!(0),
            "Bool" => json!(false),
            "String" | "Multilines" => json!(""),
            _ => Value::Null,
        }
    };
    if kind.contains("Enum.") && !raw.is_null() {
        return Ok(enum_field_value(p, d, &raw)
            .map(|value| value["id"].clone())
            .unwrap_or(Value::Null));
    }
    value_from_raw(p, d, raw)
}
fn refresh_field(p: &Project, d: &Value, f: &mut Value) -> Result<()> {
    let values = f["realEditorValues"]
        .as_array()
        .cloned()
        .or_else(|| f.get("realEditorValue").map(|v| vec![v.clone()]))
        .unwrap_or_default();
    let mut values: Vec<_> = if array(d) {
        values
    } else {
        values.into_iter().take(1).collect()
    };
    let mut out = vec![];
    let count = if array(d) { values.len() } else { 1 };
    for i in 0..count {
        if base(d).contains("Enum.")
            && let Some(value) = values.get_mut(i)
            && !value.is_null()
            && !enum_has_value(p, d, &wrapper_value(value, 2))
        {
            *value = Value::Null;
        }
        let v = values.get(i).unwrap_or(&Value::Null);
        out.push(if v.is_null() {
            default_value(p, d)?
        } else {
            value_from_raw(p, d, wrapper_value(v, 2))?
        });
    }
    f["__identifier"] = d["identifier"].clone();
    f["__type"] = d["__type"].clone();
    f["__value"] = if array(d) {
        json!(out)
    } else {
        out.into_iter().next().unwrap_or(Value::Null)
    };
    f["realEditorValues"] = json!(values);
    f["__tile"] = Value::Null;
    if d["editorDisplayMode"] == "EntityTile" || d["editorDisplayMode"] == "LevelTile" {
        let v = if array(d) {
            &f["__value"][0]
        } else {
            &f["__value"]
        };
        if base(d) == "Tile" {
            f["__tile"] = v.clone();
        } else if base(d).contains("Enum.") {
            f["__tile"] = enum_field_value(p, d, v)
                .map(|value| value["tileRect"].clone())
                .unwrap_or(Value::Null);
        }
    }
    Ok(())
}
pub(crate) fn refresh_field_values(p: &mut Project) -> Result<()> {
    for definition in field_defs(p) {
        if base(&definition).contains("Enum.")
            && !definition["defaultOverride"].is_null()
            && !enum_has_value(
                p,
                &definition,
                &wrapper_value(&definition["defaultOverride"], 1),
            )
            && let Some(uid) = definition["uid"].as_i64()
            && let Some(definition) = field_def_mut(p, uid)
        {
            definition["defaultOverride"] = Value::Null;
        }
    }
    let defs = field_defs(p);
    let context = p.clone();
    for key in p.level_keys() {
        let level = p
            .level_mut(key)
            .ok_or_else(|| invalid("level not loaded"))?;
        let sort = |fields: &mut Vec<Value>| {
            fields.sort_by_key(|f| {
                defs.iter()
                    .position(|d| d["uid"] == f["defUid"])
                    .unwrap_or(usize::MAX)
            });
        };
        if let Some(fields) = level["fieldInstances"].as_array_mut() {
            sort(fields);
        }
        if let Some(layers) = level["layerInstances"].as_array_mut() {
            for layer in layers {
                if let Some(es) = layer["entityInstances"].as_array_mut() {
                    for e in es {
                        if let Some(fields) = e["fieldInstances"].as_array_mut() {
                            sort(fields);
                        }
                    }
                }
            }
        }
        let mut error = None;
        visit_fields_mut(level, |f| {
            if let Some(d) = defs.iter().find(|d| d["uid"] == f["defUid"])
                && let Err(e) = refresh_field(&context, d, f)
            {
                error = Some(e);
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
    }
    Ok(())
}
fn refresh_fields(p: &mut Project) -> Result<()> {
    p.refresh_derived_data()
}
fn fields_at_mut<'a>(
    p: &'a mut Project,
    key: LevelKey,
    entity: Option<&str>,
) -> Result<&'a mut Vec<Value>> {
    let level = p
        .level_mut(key)
        .ok_or_else(|| invalid("level not loaded"))?;
    if let Some(iid) = entity {
        let layers = level["layerInstances"]
            .as_array_mut()
            .ok_or_else(|| invalid("layers not loaded"))?;
        for l in layers {
            if let Some(entities) = l["entityInstances"].as_array_mut() {
                for e in entities {
                    if e["iid"] == iid {
                        return e["fieldInstances"]
                            .as_array_mut()
                            .ok_or_else(|| invalid("entity fields missing"));
                    }
                }
            }
        }
        Err(invalid("entity not found"))
    } else {
        level["fieldInstances"]
            .as_array_mut()
            .ok_or_else(|| invalid("level fields missing"))
    }
}
impl Project {
    pub fn make_field_instance(&self, d: &Value) -> Result<Value> {
        let mut f = json!({"defUid":d["uid"],"__identifier":d["identifier"],"__type":d["__type"],"__value":null,"__tile":null,"realEditorValues":[]});
        refresh_field(self, d, &mut f)?;
        Ok(f)
    }
    pub fn set_field_value(
        &mut self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
        value: Value,
    ) -> Result<()> {
        self.set_field_input(key, entity, uid, value, true)
    }

    /// Apply a committed editor value. Array bounds are reported as errors in the
    /// form, so users can build or remove an array one item at a time.
    pub fn set_field_editor_value(
        &mut self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
        value: Value,
    ) -> Result<()> {
        self.set_field_input(key, entity, uid, value, false)
    }

    /// Edit one array slot without materializing defaults in untouched slots.
    /// Length constraints are diagnostics, as in the source field editor.
    pub fn edit_field_array(
        &mut self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
        edit: FieldArrayEdit,
    ) -> Result<()> {
        self.edit_document(|p| {
            let d = field_def(p, uid)?;
            if !array(&d) {
                return Err(invalid("field is not an array"));
            }
            let mut f = fields_at_mut(p, key, entity)?
                .iter()
                .find(|f| f["defUid"] == uid)
                .cloned()
                .ok_or_else(|| invalid("field instance not found"))?;
            let old = f.clone();
            let values = f["realEditorValues"]
                .as_array_mut()
                .ok_or_else(|| invalid("field editor values missing"))?;
            let insert = matches!(edit, FieldArrayEdit::InsertValue { .. });
            match edit {
                FieldArrayEdit::Set { index, value }
                | FieldArrayEdit::InsertValue { index, value } => {
                    if index > values.len() || !insert && index == values.len() {
                        return Err(invalid("array index outside field"));
                    }
                    let mut value = value;
                    // The source file picker uses slashes before parsing text escapes.
                    if base(&d) == "FilePath"
                        && let (Some(path), Some(base)) =
                            (value.as_str(), p.path.as_ref().and_then(|p| p.parent()))
                        && std::path::Path::new(path.trim()).is_absolute()
                    {
                        value = json!(crate::project::relative_to(
                            std::path::Path::new(path.trim()),
                            base
                        )?);
                    }
                    let value = normalize_input(&d, value, true)?;
                    if base(&d) == "EntityRef" {
                        validate_refs(p, key, entity, &d, std::slice::from_ref(&value))?;
                    }
                    let value = wrapper(&d, &value, 2)?;
                    if insert {
                        values.insert(index, value);
                    } else {
                        values[index] = value;
                    }
                }
                FieldArrayEdit::Insert { index } => {
                    if index > values.len() {
                        return Err(invalid("array index outside field"));
                    }
                    values.insert(index, Value::Null);
                }
                FieldArrayEdit::Remove { index } => {
                    if index >= values.len() {
                        return Err(invalid("array index outside field"));
                    }
                    values.remove(index);
                }
                FieldArrayEdit::Move { from, to } => {
                    if from >= values.len() || to >= values.len() {
                        return Err(invalid("array index outside field"));
                    }
                    let value = values.remove(from);
                    values.insert(to, value);
                }
                FieldArrayEdit::Reset { index } => {
                    let value = values
                        .get_mut(index)
                        .ok_or_else(|| invalid("array index outside field"))?;
                    *value = Value::Null;
                }
            }
            refresh_field(p, &d, &mut f)?;
            *fields_at_mut(p, key, entity)?
                .iter_mut()
                .find(|f| f["defUid"] == uid)
                .expect("validated field") = f.clone();
            if base(&d) == "EntityRef"
                && d["symmetricalRef"] == true
                && let Some(source) = entity
            {
                symmetry(p, source, &d, &old, &f)?;
                tidy_symmetric_refs(p)?;
            }
            refresh_fields(p)
        })
    }

    fn set_field_input(
        &mut self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
        value: Value,
        enforce_length: bool,
    ) -> Result<()> {
        self.edit_document(|p| {
            let d = field_def(p, uid)?;
            let mut values = if array(&d) {
                let a = value
                    .as_array()
                    .ok_or_else(|| invalid("array field requires an array"))?;
                if enforce_length
                    && (d["arrayMinLength"]
                        .as_u64()
                        .is_some_and(|n| a.len() < (n as usize))
                        || d["arrayMaxLength"]
                            .as_u64()
                            .is_some_and(|n| a.len() > (n as usize)))
                {
                    return Err(invalid("array length outside definition limits"));
                }
                a.clone()
            } else {
                vec![value]
            };
            for v in &mut values {
                if base(&d) == "FilePath"
                    && let (Some(path), Some(base)) =
                        (v.as_str(), p.path.as_ref().and_then(|p| p.parent()))
                    && std::path::Path::new(path.trim()).is_absolute()
                {
                    *v = json!(crate::project::relative_to(
                        std::path::Path::new(path.trim()),
                        base
                    )?);
                }
                *v = normalize_input(&d, v.clone(), true)?;
            }
            if base(&d) == "EntityRef" {
                validate_refs(p, key, entity, &d, &values)?;
            }
            let wrappers = values
                .iter()
                .map(|v| wrapper(&d, v, 2))
                .collect::<Result<Vec<_>>>()?;
            let mut f = fields_at_mut(p, key, entity)?
                .iter()
                .find(|f| f["defUid"] == uid)
                .cloned()
                .ok_or_else(|| invalid("field instance not found"))?;
            let old = f.clone();
            f["realEditorValues"] = json!(wrappers);
            refresh_field(p, &d, &mut f)?;
            *fields_at_mut(p, key, entity)?
                .iter_mut()
                .find(|f| f["defUid"] == uid)
                .expect("validated field") = f.clone();
            if base(&d) == "EntityRef"
                && d["symmetricalRef"] == true
                && let Some(source) = entity
            {
                symmetry(p, source, &d, &old, &f)?;
                tidy_symmetric_refs(p)?;
            }
            refresh_fields(p)
        })
    }
    /// Source-compatible form diagnostics; invalid semantic values remain editable.
    pub fn field_value_errors(
        &self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
    ) -> Result<Vec<String>> {
        let d = field_def(self, uid)?;
        let level = self.level(key).ok_or_else(|| invalid("level not loaded"))?;
        let owner = if let Some(iid) = entity {
            level["layerInstances"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|l| l["entityInstances"].as_array().into_iter().flatten())
                .find(|e| e["iid"] == iid)
                .ok_or_else(|| invalid("entity not found"))?
        } else {
            level
        };
        let field = owner["fieldInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["defUid"] == uid)
            .ok_or_else(|| invalid("field instance not found"))?;
        let values = if array(&d) {
            field["__value"].as_array().cloned().unwrap_or_default()
        } else {
            vec![field["__value"].clone()]
        };
        let mut errors = Vec::new();
        if array(&d) {
            if d["arrayMinLength"]
                .as_u64()
                .is_some_and(|n| values.len() < n as usize)
            {
                errors.push("Array too short".into());
            }
            if d["arrayMaxLength"]
                .as_u64()
                .is_some_and(|n| values.len() > n as usize)
            {
                errors.push("Array too long".into());
            }
        }
        let kind = base(&d);
        for value in &values {
            let error = if value.is_null() {
                if d["canBeNull"] != true
                    && (matches!(kind, "Point" | "FilePath" | "EntityRef" | "Tile")
                        || kind.contains("Enum."))
                {
                    Some("Value required".to_owned())
                } else {
                    None
                }
            } else if kind == "FilePath" {
                let exists = self
                    .make_absolute_file_path(value.as_str().unwrap_or(""))
                    .is_ok_and(|p| p.exists());
                (!exists).then(|| "File not found".to_owned())
            } else if kind == "EntityRef" {
                let target = value["entityIid"]
                    .as_str()
                    .and_then(|id| find_entity(self, id));
                match target {
                    None => Some("Lost reference!".to_owned()),
                    Some((_, target)) => {
                        let target_def =
                            self.definition("entities", target["defUid"].as_i64().unwrap_or(-1));
                        match d["allowedRefs"].as_str().unwrap_or("Any") {
                            "OnlySame"
                                if entity.is_some() && owner["defUid"] != target["defUid"] =>
                            {
                                Some(format!(
                                    "Invalid ref type {}",
                                    target["__identifier"].as_str().unwrap_or("")
                                ))
                            }
                            "OnlySpecificEntity"
                                if target["defUid"] != d["allowedRefsEntityUid"] =>
                            {
                                Some(format!(
                                    "Invalid ref type {}",
                                    target["__identifier"].as_str().unwrap_or("")
                                ))
                            }
                            "OnlyTags"
                                if !d["allowedRefTags"].as_array().is_some_and(|tags| {
                                    tags.iter().any(|tag| {
                                        target_def.is_some_and(|def| {
                                            def["tags"]
                                                .as_array()
                                                .is_some_and(|ts| ts.contains(tag))
                                        })
                                    })
                                }) =>
                            {
                                Some("Invalid ref tags".into())
                            }
                            _ => None,
                        }
                    }
                }
            } else {
                None
            };
            if let Some(error) = error {
                errors.push(error);
            }
        }
        Ok(errors)
    }

    pub fn reset_field_value(
        &mut self,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
    ) -> Result<()> {
        self.edit_document(|p| {
            let d = field_def(p, uid)?;
            let f = fields_at_mut(p, key, entity)?
                .iter_mut()
                .find(|f| f["defUid"] == uid)
                .ok_or_else(|| invalid("field instance not found"))?;
            let old = f.clone();
            f["realEditorValues"] = json!([]);
            let new = f.clone();
            if base(&d) == "EntityRef"
                && d["symmetricalRef"] == true
                && let Some(source) = entity
            {
                symmetry(p, source, &d, &old, &new)?;
                tidy_symmetric_refs(p)?;
            }
            refresh_fields(p)
        })
    }
    pub fn add_field_definition(
        &mut self,
        entity_uid: Option<i64>,
        name: &str,
        kind: &str,
        is_array: bool,
    ) -> Result<i64> {
        self.edit_document(|p|{if !valid_identifier(name){return Err(invalid("invalid field identifier"));}let (public,internal)=type_data(p,kind,is_array)?;let defs=if let Some(uid)=entity_uid{p.definition("entities",uid).ok_or_else(||invalid("entity definition not found"))?["fieldDefs"].as_array()}else{p.json["defs"]["levelFields"].as_array()}.ok_or_else(||invalid("field definitions missing"))?;if defs.iter().any(|f|f["identifier"]==name){return Err(invalid("field identifier already exists"));}let uid=alloc_uid(p)?;let nullable=["String","Multilines","FilePath","Point"].contains(&kind)||(kind=="EntityRef"&&!is_array);let d=json!({"identifier":name,"doc":null,"__type":public,"uid":uid,"type":internal,"isArray":is_array,"canBeNull":nullable,"arrayMinLength":null,"arrayMaxLength":null,"editorDisplayMode":match kind{"Point"=>"PointPath","EntityRef"=>"RefLinkBetweenCenters","Tile"=>"EntityTile",_=>"Hidden"},"editorDisplayPos":"Above","editorDisplayScale":1,"editorLinkStyle":if kind=="EntityRef"{"CurvedArrow"}else{"StraightArrow"},"editorAlwaysShow":false,"editorShowInWorld":true,"editorCutLongValues":true,"editorTextSuffix":null,"editorTextPrefix":null,"useForSmartColor":kind=="Color"||kind.contains("Enum."),"min":null,"max":null,"regex":null,"acceptFileTypes":null,"defaultOverride":null,"textLanguageMode":null,"symmetricalRef":false,"autoChainRef":true,"allowOutOfLevelRef":true,"allowedRefs":"OnlySame","allowedRefsEntityUid":null,"allowedRefTags":[],"tilesetUid":null,"exportToToc":false,"searchable":false});let f=p.make_field_instance(&d)?;
 if let Some(owner)=entity_uid{let e=p.json["defs"]["entities"].as_array_mut().expect("validated definitions").iter_mut().find(|e|e["uid"]==owner).expect("validated entity");e["fieldDefs"].as_array_mut().expect("validated fields").push(d);for key in p.level_keys(){let level=p.level_mut(key).ok_or_else(||invalid("level not loaded"))?;if let Some(layers)=level["layerInstances"].as_array_mut(){for l in layers{if let Some(es)=l["entityInstances"].as_array_mut(){for e in es{if e["defUid"]==owner{e["fieldInstances"].as_array_mut().ok_or_else(||invalid("entity fields missing"))?.push(f.clone());}}}}}}}else{p.json["defs"]["levelFields"].as_array_mut().expect("validated fields").push(d);for key in p.level_keys(){fields_at_mut(p,key,None)?.push(f.clone());}}p.refresh_derived_data()?;Ok(uid)})
    }
    pub fn set_field_default(&mut self, uid: i64, value: Value) -> Result<()> {
        self.edit_document(|p| {
            let d = field_def(p, uid)?;
            let value = normalize_input(&d, value, false)?;
            let w = wrapper(&d, &value, 1)?;
            field_def_mut(p, uid).expect("validated field")["defaultOverride"] = w;
            refresh_fields(p)
        })
    }
    pub fn update_field_definition(&mut self, uid: i64, patch: &Value) -> Result<()> {
        self.edit_document(|p| {
            let patch = patch
                .as_object()
                .ok_or_else(|| invalid("field patch must be an object"))?;
            if patch
                .keys()
                .any(|k| ["uid", "type", "__type", "isArray"].contains(&k.as_str()))
            {
                return Err(invalid("field type requires explicit conversion"));
            }
            if let Some(name) = patch.get("identifier").and_then(Value::as_str)
                && !valid_identifier(name)
            {
                return Err(invalid("invalid field identifier"));
            }
            if let Some(name) = patch.get("identifier").and_then(Value::as_str) {
                let owners = p.json["defs"]["entities"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let fields = owners
                    .iter()
                    .find(|e| {
                        e["fieldDefs"]
                            .as_array()
                            .is_some_and(|fs| fs.iter().any(|f| f["uid"] == uid))
                    })
                    .map(|e| e["fieldDefs"].clone())
                    .unwrap_or_else(|| p.json["defs"]["levelFields"].clone());
                if fields.as_array().is_some_and(|fs| {
                    fs.iter()
                        .any(|f| f["uid"] != uid && f["identifier"] == name)
                }) {
                    return Err(invalid("field identifier already exists"));
                }
            }
            if let Some(pattern) = patch.get("regex").and_then(Value::as_str) {
                compile_regex(pattern)?;
            }
            let d = field_def_mut(p, uid).ok_or_else(|| invalid("field definition not found"))?;
            for (k, v) in patch {
                d[k] = v.clone();
            }
            if d["min"]
                .as_f64()
                .zip(d["max"].as_f64())
                .is_some_and(|(a, b)| a > b)
            {
                let old_min = d["min"].clone();
                d["min"] = d["max"].clone();
                d["max"] = old_min;
            }
            if ["Int", "Float"].contains(&base(d)) && !d["defaultOverride"].is_null() {
                let raw = wrapper_value(&d["defaultOverride"], 1);
                let normalized = normalize_input(d, raw, false)?;
                d["defaultOverride"] = wrapper(d, &normalized, 1)?;
            }
            refresh_fields(p)
        })
    }
    pub fn remove_field_definition(&mut self, uid: i64) -> Result<()> {
        self.edit_document(|p| {
            field_def(p, uid)?;
            if let Some(defs) = p.json["defs"]["levelFields"].as_array_mut() {
                defs.retain(|d| d["uid"] != uid);
            }
            if let Some(es) = p.json["defs"]["entities"].as_array_mut() {
                for e in es {
                    if let Some(fs) = e["fieldDefs"].as_array_mut() {
                        fs.retain(|d| d["uid"] != uid);
                    }
                }
            }
            for key in p.level_keys() {
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                if let Some(fs) = l["fieldInstances"].as_array_mut() {
                    fs.retain(|f| f["defUid"] != uid);
                }
                if let Some(ls) = l["layerInstances"].as_array_mut() {
                    for l in ls {
                        if let Some(es) = l["entityInstances"].as_array_mut() {
                            for e in es {
                                if let Some(fs) = e["fieldInstances"].as_array_mut() {
                                    fs.retain(|f| f["defUid"] != uid);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(layers) = p.json["defs"]["layers"].as_array_mut() {
                for layer in layers {
                    if layer["biomeFieldUid"] == uid {
                        layer["biomeFieldUid"] = Value::Null;
                        if let Some(groups) = layer["autoRuleGroups"].as_array_mut() {
                            for group in groups {
                                group["requiredBiomeValues"] = json!([]);
                                group["biomeRequirementMode"] = json!(0);
                            }
                        }
                    }
                }
            }
            p.refresh_auto_and_derived()
        })
    }
    /// Conversions available in original FieldTypeConverter; `round` chooses rounding over truncation.
    pub fn convert_field_type(
        &mut self,
        uid: i64,
        kind: &str,
        is_array: bool,
        round: bool,
    ) -> Result<()> {
        self.edit_document(|p| {
            let old = field_def(p, uid)?;
            let from = base(&old);
            if array(&old) && !is_array {
                return Err(invalid("array to scalar would discard values"));
            }
            if from != kind
                && !matches!(
                    (from, kind),
                    ("String", "Multilines")
                        | ("Multilines", "String")
                        | ("FilePath", "String")
                        | ("String", "FilePath")
                        | ("Int", "Float")
                        | ("Bool", "Int")
                        | ("Float", "Int")
                )
            {
                return Err(Error::Unsupported(format!(
                    "field conversion {from} to {kind}"
                )));
            }
            let (public, internal) = type_data(p, kind, is_array)?;
            let d = field_def_mut(p, uid).expect("validated field");
            d["__type"] = json!(public);
            d["type"] = internal;
            d["isArray"] = json!(is_array);
            let new = d.clone();
            let convert = |w: &Value, depth| -> Result<Value> {
                if w.is_null() {
                    return Ok(Value::Null);
                }
                let raw = wrapper_value(w, depth);
                let raw = match (from, kind) {
                    ("Bool", "Int") => json!(i64::from(raw.as_bool().unwrap_or(false))),
                    ("Float", "Int") => {
                        let n = raw.as_f64().ok_or_else(|| invalid("invalid Float value"))?;
                        json!(if round {
                            (n + 0.5).floor() as i64
                        } else {
                            n as i64
                        })
                    }
                    ("Multilines", "String") => {
                        json!(raw.as_str().unwrap_or("").replace('\n', " "))
                    }
                    _ => raw,
                };
                wrapper(&new, &raw, depth)
            };
            let default = convert(&old["defaultOverride"], 1)?;
            field_def_mut(p, uid).expect("validated field")["defaultOverride"] = default;
            for key in p.level_keys() {
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                let mut err = None;
                visit_fields_mut(l, |f| {
                    if f["defUid"] == uid
                        && let Some(vs) = f["realEditorValues"].as_array_mut()
                    {
                        for v in vs {
                            match convert(v, 2) {
                                Ok(new) => *v = new,
                                Err(e) => err = Some(e),
                            }
                        }
                    }
                });
                if let Some(e) = err {
                    return Err(e);
                }
            }
            refresh_fields(p)
        })
    }
    pub fn add_enum_definition(&mut self, name: &str, values: &[&str]) -> Result<i64> {
        self.edit_document(|p|{if !valid_identifier(name)||values.iter().any(|s|!valid_identifier(s)){return Err(invalid("invalid enum identifier"));}let mut sorted=values.to_vec();sorted.sort_unstable();sorted.dedup();if sorted.len()!=values.len(){return Err(invalid("duplicate enum value"));}let es=p.json["defs"]["enums"].as_array().ok_or_else(||invalid("enum definitions missing"))?;if es.iter().any(|e|e["identifier"]==name){return Err(invalid("enum identifier already exists"));}let uid=alloc_uid(p)?;p.json["defs"]["enums"].as_array_mut().expect("validated enums").push(json!({"uid":uid,"identifier":name,"iconTilesetUid":null,"externalRelPath":null,"externalFileChecksum":null,"tags":[],"values":values.iter().map(|id|json!({"id":id,"tileRect":null,"color":null})).collect::<Vec<_>>()}));Ok(uid)})
    }
    pub fn rename_enum_value(&mut self, uid: i64, old: &str, new: &str) -> Result<()> {
        self.rename_enum_value_to(uid, old, new, true)
    }

    /// Resolve an identifier typed in the enum panel using the project's naming convention.
    pub fn rename_enum_value_from_editor(
        &mut self,
        uid: i64,
        old: &str,
        input: &str,
    ) -> Result<String> {
        if input.is_empty() {
            return Err(invalid("invalid enum value identifier"));
        }
        let values = self.json["defs"]["enums"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["uid"] == uid)
            .and_then(|e| e["values"].as_array())
            .ok_or_else(|| invalid("enum definition missing"))?;
        if !values.iter().any(|value| value["id"] == old) {
            return Err(invalid("enum value missing"));
        }
        let occupied = values
            .iter()
            .filter_map(|value| value["id"].as_str())
            .filter(|id| *id != old)
            .map(str::to_owned)
            .collect();
        let style = self.json["identifierStyle"]
            .as_str()
            .unwrap_or("Capitalize");
        let new = crate::naming::unique_name(input, style, &occupied);
        if new != old {
            // LDtk's panel rewrites explicit instances and leaves the stored default override.
            self.rename_enum_value_to(uid, old, &new, false)?;
        }
        Ok(new)
    }

    fn rename_enum_value_to(
        &mut self,
        uid: i64,
        old: &str,
        new: &str,
        update_defaults: bool,
    ) -> Result<()> {
        self.edit_document(|p| {
            if !valid_identifier(new) {
                return Err(invalid("invalid enum value identifier"));
            }
            let e = p.json["defs"]["enums"]
                .as_array_mut()
                .and_then(|es| es.iter_mut().find(|e| e["uid"] == uid))
                .ok_or_else(|| invalid("enum definition missing"))?;
            let name = e["identifier"].as_str().unwrap_or("").to_owned();
            let values = e["values"]
                .as_array_mut()
                .ok_or_else(|| invalid("enum values missing"))?;
            if values.iter().any(|v| v["id"] == new && v["id"] != old) {
                return Err(invalid("enum value already exists"));
            }
            let v = values
                .iter_mut()
                .find(|v| v["id"] == old)
                .ok_or_else(|| invalid("enum value missing"))?;
            v["id"] = json!(new);
            let kind = format!("LocalEnum.{name}");
            let mut uids = Vec::new();
            let mut preserved_defaults = Vec::new();
            for mut field in field_defs(p) {
                let Some(uid) = field["uid"].as_i64() else {
                    continue;
                };
                let field_kind = base(&field);
                let affected = field_kind == kind;
                let default = &field["defaultOverride"];
                if !update_defaults && field_kind.contains("Enum.") && !default.is_null() {
                    preserved_defaults.push((uid, std::mem::take(&mut field["defaultOverride"])));
                } else if update_defaults && affected && wrapper_value(default, 1) == old {
                    field_def_mut(p, uid).expect("found field")["defaultOverride"]["params"][0] =
                        json!(escape(new));
                }
                if affected {
                    uids.push(uid);
                }
            }
            for key in p.level_keys() {
                let l = p.level_mut(key).ok_or_else(|| invalid("level missing"))?;
                visit_fields_mut(l, |f| {
                    if uids.iter().any(|uid| f["defUid"] == *uid)
                        && let Some(vs) = f["realEditorValues"].as_array_mut()
                    {
                        for v in vs {
                            if wrapper_value(v, 2) == old {
                                v["params"][0] = json!(escape(&escape(new)));
                            }
                        }
                    }
                });
            }
            update_enum_uses(p, uid, &uids, old, Some(new));
            p.refresh_auto_and_derived()?;
            // The panel leaves the invalid default in the definition until the next tidy.
            // Derived instance values already reflect its absence.
            for (uid, default) in preserved_defaults {
                field_def_mut(p, uid).expect("found field")["defaultOverride"] = default;
            }
            Ok(())
        })
    }
    /// Whether deleting this local enum value needs confirmation in the editor.
    pub fn is_enum_value_used(&self, uid: i64, value: &str) -> bool {
        fn items(value: &Value) -> &[Value] {
            value.as_array().map_or(&[], Vec::as_slice)
        }

        let defs = &self.json["defs"];
        let Some(definition) = items(&defs["enums"]).iter().find(|e| e["uid"] == uid) else {
            return false;
        };
        let kind = format!(
            "LocalEnum.{}",
            definition["identifier"].as_str().unwrap_or("")
        );
        let fields_use_value = |fields: &Value| {
            items(fields).iter().any(|field| {
                let current = &field["__value"];
                base(field) == kind
                    && (current == value || items(current).iter().any(|v| v == value))
            })
        };
        self.level_keys()
            .into_iter()
            .filter_map(|key| self.level(key))
            .any(|level| {
                fields_use_value(&level["fieldInstances"])
                    || items(&level["layerInstances"])
                        .iter()
                        .flat_map(|layer| items(&layer["entityInstances"]))
                        .any(|entity| fields_use_value(&entity["fieldInstances"]))
            })
            || items(&defs["layers"]).iter().any(|layer| {
                let biome_field = &layer["biomeFieldUid"];
                biome_field.is_number()
                    && items(&defs["levelFields"])
                        .iter()
                        .any(|field| field["uid"] == *biome_field && base(field) == kind)
                    && items(&layer["autoRuleGroups"])
                        .iter()
                        .flat_map(|group| items(&group["requiredBiomeValues"]))
                        .any(|v| v == value)
            })
    }
    pub fn remove_enum_value(&mut self, uid: i64, value: &str) -> Result<()> {
        self.edit_document(|p| {
            let e = p.json["defs"]["enums"]
                .as_array_mut()
                .and_then(|es| es.iter_mut().find(|e| e["uid"] == uid))
                .ok_or_else(|| invalid("enum definition missing"))?;
            let name = e["identifier"].as_str().unwrap_or("").to_owned();
            let vs = e["values"]
                .as_array_mut()
                .ok_or_else(|| invalid("enum values missing"))?;
            let before = vs.len();
            vs.retain(|v| v["id"] != value);
            if before == vs.len() {
                return Err(invalid("enum value missing"));
            }
            let uids: Vec<_> = field_defs(p)
                .into_iter()
                .filter(|d| base(d) == format!("LocalEnum.{name}"))
                .filter_map(|d| d["uid"].as_i64())
                .collect();
            for uid in &uids {
                let d = field_def_mut(p, *uid).expect("found field");
                if wrapper_value(&d["defaultOverride"], 1) == value {
                    d["defaultOverride"] = Value::Null;
                }
            }
            for key in p.level_keys() {
                let l = p.level_mut(key).ok_or_else(|| invalid("level missing"))?;
                visit_fields_mut(l, |f| {
                    if uids.iter().any(|uid| f["defUid"] == *uid)
                        && let Some(vs) = f["realEditorValues"].as_array_mut()
                    {
                        for v in vs {
                            if wrapper_value(v, 2) == value {
                                *v = Value::Null;
                            }
                        }
                    }
                });
            }
            update_enum_uses(p, uid, &uids, value, None);
            p.refresh_auto_and_derived()
        })
    }
}
impl Project {
    pub(crate) fn rename_enum_type_cache(
        &mut self,
        kind: &str,
        old: &str,
        new: &str,
    ) -> Result<()> {
        let prefix = if kind == "externalEnums" {
            "ExternEnum"
        } else {
            "LocalEnum"
        };
        let old_type = format!("{prefix}.{old}");
        let new_type = format!("{prefix}.{new}");
        let defs = field_defs(self);
        for d in defs {
            if base(&d) == old_type {
                let uid = d["uid"]
                    .as_i64()
                    .ok_or_else(|| invalid("field UID missing"))?;
                field_def_mut(self, uid).expect("found field")["__type"] = json!(if array(&d) {
                    format!("Array<{new_type}>")
                } else {
                    new_type.clone()
                });
            }
        }
        refresh_fields(self)
    }
    pub fn update_enum_definition(&mut self, uid: i64, patch: &Value) -> Result<()> {
        self.edit_document(|p| {
            let fields = patch
                .as_object()
                .ok_or_else(|| invalid("enum patch must be an object"))?;
            if fields.contains_key("uid") {
                return Err(invalid("enum UID is immutable"));
            }
            if let Some(values) = patch.get("values") {
                let values = values
                    .as_array()
                    .ok_or_else(|| invalid("enum values must be an array"))?;
                let removed: Vec<_> = p
                    .definition("enums", uid)
                    .ok_or_else(|| invalid("enum definition missing"))?["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|old| {
                        old["id"]
                            .as_str()
                            .filter(|id| !values.iter().any(|new| new["id"] == *id))
                            .map(str::to_owned)
                    })
                    .collect();
                for value in removed {
                    p.remove_enum_value(uid, &value)?;
                }
            }
            if let Some(name) = patch["identifier"].as_str() {
                p.rename_definition("enums", uid, name)?;
            }
            let d = p.json["defs"]["enums"]
                .as_array_mut()
                .and_then(|ds| ds.iter_mut().find(|d| d["uid"] == uid))
                .ok_or_else(|| invalid("enum definition missing"))?;
            for (k, v) in fields {
                d[k] = v.clone();
            }
            let values = d["values"]
                .as_array()
                .ok_or_else(|| invalid("enum values must be an array"))?;
            let mut names = std::collections::BTreeSet::new();
            for value in values {
                let id = value["id"]
                    .as_str()
                    .filter(|s| valid_identifier(s))
                    .ok_or_else(|| invalid("invalid enum value identifier"))?;
                if !names.insert(id) {
                    return Err(invalid("duplicate enum value identifier"));
                }
            }
            refresh_fields(p)
        })
    }
}
fn find_entity(p: &Project, iid: &str) -> Option<(LevelKey, Value)> {
    for key in p.level_keys() {
        let level = p.level(key)?;
        if let Some(ls) = level["layerInstances"].as_array() {
            for l in ls {
                if let Some(es) = l["entityInstances"].as_array() {
                    for e in es {
                        if e["iid"] == iid {
                            return Some((key, e.clone()));
                        }
                    }
                }
            }
        }
    }
    None
}
fn validate_refs(
    p: &Project,
    key: LevelKey,
    source: Option<&str>,
    d: &Value,
    values: &[Value],
) -> Result<()> {
    let source_def = source
        .and_then(|s| find_entity(p, s))
        .map(|(_, e)| e["defUid"].clone());
    for v in values {
        if v.is_null() {
            continue;
        }
        let id = v
            .as_str()
            .or_else(|| v["entityIid"].as_str())
            .ok_or_else(|| invalid("EntityRef requires an IID"))?;
        let (target_key, target) =
            find_entity(p, id).ok_or_else(|| invalid("referenced entity does not exist"))?;
        if d["allowOutOfLevelRef"] == false && target_key != key {
            return Err(invalid("field disallows references outside this level"));
        }
        let target_def = p
            .definition("entities", target["defUid"].as_i64().unwrap_or(-1))
            .ok_or_else(|| invalid("referenced entity definition missing"))?;
        match d["allowedRefs"].as_str().unwrap_or("Any") {
            "OnlySame" => {
                if source_def.as_ref().is_some_and(|s| *s != target["defUid"]) {
                    return Err(invalid("field only allows entities of the same type"));
                }
            }
            "OnlySpecificEntity" => {
                if target["defUid"] != d["allowedRefsEntityUid"] {
                    return Err(invalid("referenced entity type is not allowed"));
                }
            }
            "OnlyTags"
                if !d["allowedRefTags"].as_array().is_some_and(|tags| {
                    tags.iter().any(|tag| {
                        target_def["tags"]
                            .as_array()
                            .is_some_and(|ts| ts.contains(tag))
                    })
                }) =>
            {
                return Err(invalid("referenced entity tags are not allowed"));
            }
            _ => {}
        }
    }
    Ok(())
}
fn wrapper_ids(f: &Value) -> Vec<String> {
    f["realEditorValues"]
        .as_array()
        .map(|vs| {
            vs.iter()
                .filter_map(|v| wrapper_value(v, 2).as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
fn symmetry(p: &mut Project, source: &str, d: &Value, old: &Value, new: &Value) -> Result<()> {
    let uid = d["uid"]
        .as_i64()
        .ok_or_else(|| invalid("field UID missing"))?;
    let old_ids = wrapper_ids(old);
    let new_ids = wrapper_ids(new);
    for target in old_ids
        .iter()
        .chain(new_ids.iter())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let Some((key, _)) = find_entity(p, target) else {
            continue;
        };
        let fields = fields_at_mut(p, key, Some(target))?;
        let Some(f) = fields.iter_mut().find(|f| f["defUid"] == uid) else {
            continue;
        };
        let values = f["realEditorValues"]
            .as_array_mut()
            .ok_or_else(|| invalid("target editor values missing"))?;
        let contains = values.iter().any(|v| wrapper_value(v, 2) == source);
        if new_ids.contains(target) {
            if !contains {
                let w = wrapper(d, &json!(source), 2)?;
                if array(d) {
                    values.push(w);
                } else {
                    *values = vec![w];
                }
            }
        } else if contains {
            if array(d) {
                values.retain(|v| wrapper_value(v, 2) != source);
            } else {
                values.clear();
            }
        }
    }
    Ok(())
}
impl Project {
    pub(crate) fn remove_enum_field_definitions(&mut self, name: &str, kind: &str) -> Result<()> {
        let public = format!(
            "{}.{name}",
            if kind == "enums" {
                "LocalEnum"
            } else {
                "ExternEnum"
            }
        );
        let fields: Vec<_> = field_defs(self)
            .into_iter()
            .filter(|d| base(d) == public)
            .filter_map(|d| d["uid"].as_i64())
            .collect();
        for uid in fields {
            self.remove_field_definition(uid)?;
        }
        Ok(())
    }
}
fn compile_regex(pattern: &str) -> Result<regress::Regex> {
    let (body, flags) = pattern
        .strip_prefix('/')
        .and_then(|s| s.rsplit_once('/'))
        .ok_or_else(|| invalid("regex must use /pattern/flags"))?;
    if flags.chars().any(|c| !"dgimsuvy".contains(c)) {
        return Err(invalid("unsupported regex flag"));
    }
    regress::Regex::with_flags(body, flags)
        .map_err(|e| invalid(&format!("invalid field regex: {e}")))
}
fn apply_regex(pattern: &str, text: &str) -> Result<String> {
    let regex = compile_regex(pattern)?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut remaining = units.as_slice();
    let mut output = vec![];
    let sticky = pattern
        .rsplit_once('/')
        .is_some_and(|(_, flags)| flags.contains('y'));
    while let Some(found) = regex.find_from_utf16(remaining, 0).next() {
        if sticky && found.range.start != 0 {
            break;
        }
        output.extend_from_slice(&remaining[found.range.clone()]);
        if found.range.end == 0 {
            break;
        }
        remaining = &remaining[found.range.end..];
        if remaining.is_empty() {
            break;
        }
    }
    String::from_utf16(&output).map_err(|_| invalid("regex produced an unpaired UTF-16 surrogate"))
}
fn normalize_input(d: &Value, mut value: Value, parse: bool) -> Result<Value> {
    if value.is_null() {
        return Ok(value);
    }
    match base(d) {
        "Int" | "Float" => {
            let mut n = value
                .as_f64()
                .ok_or_else(|| invalid("numeric field requires a number"))?;
            if let Some(min) = d["min"].as_f64() {
                n = n.max(if base(d) == "Int" {
                    (min + 0.5).floor()
                } else {
                    min
                });
            }
            if let Some(max) = d["max"].as_f64() {
                n = n.min(if base(d) == "Int" {
                    (max + 0.5).floor()
                } else {
                    max
                });
            }
            if base(d) == "Int" {
                if value.as_i64().is_none() {
                    return Err(invalid("Int field requires an integer"));
                }
                value = json!(n as i64);
            } else {
                value = json!(n);
            }
        }
        "String" | "FilePath" => {
            let raw = value
                .as_str()
                .ok_or_else(|| invalid("String field requires text"))?
                .trim();
            if raw.is_empty() {
                return Ok(Value::Null);
            }
            let mut raw = if parse {
                raw.replace("\\r", " ").replace("\\n", " ")
            } else {
                raw.to_owned()
            };
            if parse
                && base(d) == "String"
                && let Some(pattern) = d["regex"].as_str()
            {
                raw = apply_regex(pattern, &raw)?;
            }
            value = json!(raw);
        }
        "Multilines" if value.as_str() == Some("") => {
            value = Value::Null;
        }
        _ => {}
    }
    Ok(value)
}
pub(crate) fn offset_point_fields(
    layer: &mut Value,
    dx: i64,
    dy: i64,
    width: i64,
    height: i64,
) -> Result<()> {
    if let Some(es) = layer["entityInstances"].as_array_mut() {
        for e in es {
            if let Some(fs) = e["fieldInstances"].as_array_mut() {
                for f in fs {
                    let kind = f["__type"].as_str().unwrap_or("");
                    if kind != "Point" && kind != "Array<Point>" {
                        continue;
                    }
                    let is_array = kind.starts_with("Array<");
                    let mut out = vec![];
                    let mut wrappers = vec![];
                    for mut v in f["realEditorValues"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                    {
                        if v.is_null() {
                            if is_array {
                                out.push(Value::Null);
                                wrappers.push(v);
                            }
                            continue;
                        }
                        let raw = wrapper_value(&v, 2);
                        let coords: Vec<_> = raw
                            .as_str()
                            .ok_or_else(|| invalid("invalid Point wrapper"))?
                            .split(',')
                            .map(str::parse::<i64>)
                            .collect::<std::result::Result<_, _>>()
                            .map_err(|_| invalid("invalid Point coordinates"))?;
                        if coords.len() != 2 {
                            return Err(invalid("invalid Point coordinates"));
                        }
                        let x = coords[0] + dx;
                        let y = coords[1] + dy;
                        if x >= 0 && y >= 0 && x < width && y < height {
                            out.push(json!({"cx":x,"cy":y}));
                            if dx != 0 || dy != 0 {
                                v["params"][0] = json!(format!("{x},{y}"));
                            }
                            wrappers.push(v);
                        }
                    }
                    f["__value"] = if is_array {
                        json!(out)
                    } else {
                        out.into_iter().next().unwrap_or(Value::Null)
                    };
                    f["realEditorValues"] = json!(wrappers);
                }
            }
        }
    }
    Ok(())
}
impl Project {
    pub fn reorder_field_definition(&mut self, uid: i64, index: usize) -> Result<()> {
        self.edit_document(|p| {
            field_def(p, uid)?;
            let owner = p.json["defs"]["entities"].as_array().and_then(|es| {
                es.iter().position(|e| {
                    e["fieldDefs"]
                        .as_array()
                        .is_some_and(|fs| fs.iter().any(|f| f["uid"] == uid))
                })
            });
            let fields = if let Some(i) = owner {
                p.json["defs"]["entities"][i]["fieldDefs"].as_array_mut()
            } else {
                p.json["defs"]["levelFields"].as_array_mut()
            }
            .ok_or_else(|| invalid("field definitions missing"))?;
            if index >= fields.len() {
                return Err(invalid("field index outside definition list"));
            }
            let from = fields
                .iter()
                .position(|f| f["uid"] == uid)
                .expect("found field");
            let field = fields.remove(from);
            fields.insert(index, field);
            p.refresh_derived_data()
        })
    }
}
fn update_enum_uses(p: &mut Project, enum_uid: i64, fields: &[i64], old: &str, new: Option<&str>) {
    if let Some(layers) = p.json["defs"]["layers"].as_array_mut() {
        for layer in layers {
            if !fields.iter().any(|uid| layer["biomeFieldUid"] == *uid) {
                continue;
            }
            if let Some(groups) = layer["autoRuleGroups"].as_array_mut() {
                for group in groups {
                    if let Some(values) = group["requiredBiomeValues"].as_array_mut() {
                        for v in values.iter_mut() {
                            if *v == old {
                                *v = new.map_or(Value::Null, |v| json!(v));
                            }
                        }
                        values.retain(|v| !v.is_null());
                        if values.len() <= 1 {
                            group["biomeRequirementMode"] = json!(0);
                        }
                    }
                }
            }
        }
    }
    if let Some(tilesets) = p.json["defs"]["tilesets"].as_array_mut() {
        for tileset in tilesets {
            if tileset["tagsSourceEnumUid"] != enum_uid {
                continue;
            }
            if let Some(tags) = tileset["enumTags"].as_array_mut() {
                if let Some(new) = new {
                    for tag in tags {
                        if tag["enumValueId"] == old {
                            tag["enumValueId"] = json!(new);
                        }
                    }
                } else {
                    tags.retain(|tag| tag["enumValueId"] != old);
                }
            }
        }
    }
}
fn tidy_symmetric_refs(p: &mut Project) -> Result<()> {
    let defs = field_defs(p);
    let mut links = std::collections::BTreeMap::new();
    for key in p.level_keys() {
        let level = p.level(key).ok_or_else(|| invalid("level not loaded"))?;
        for layer in level["layerInstances"].as_array().into_iter().flatten() {
            for entity in layer["entityInstances"].as_array().into_iter().flatten() {
                let Some(iid) = entity["iid"].as_str() else {
                    continue;
                };
                for field in entity["fieldInstances"].as_array().into_iter().flatten() {
                    if let Some(uid) = field["defUid"].as_i64() {
                        links.insert((iid.to_owned(), uid), wrapper_ids(field));
                    }
                }
            }
        }
    }
    for key in p.level_keys() {
        let level = p
            .level_mut(key)
            .ok_or_else(|| invalid("level not loaded"))?;
        for layer in level["layerInstances"].as_array_mut().into_iter().flatten() {
            for entity in layer["entityInstances"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                let Some(iid) = entity["iid"].as_str().map(str::to_owned) else {
                    continue;
                };
                for field in entity["fieldInstances"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                {
                    let Some(uid) = field["defUid"].as_i64() else {
                        continue;
                    };
                    if !defs.iter().any(|d| {
                        d["uid"] == uid && d["symmetricalRef"] == true && base(d) == "EntityRef"
                    }) {
                        continue;
                    }
                    if let Some(values) = field["realEditorValues"].as_array_mut() {
                        values.retain(|v| {
                            let raw = wrapper_value(v, 2);
                            raw.as_str().is_none_or(|target| {
                                links
                                    .get(&(target.to_owned(), uid))
                                    .is_some_and(|refs| refs.contains(&iid))
                            })
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
