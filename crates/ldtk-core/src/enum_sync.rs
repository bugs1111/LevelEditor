//! External enum parsing and transactional synchronization.
use crate::imports::clean_identifier;
use crate::project::{absolute, relative_to};
use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedExternalEnum {
    pub identifier: String,
    pub values: Vec<Value>,
    pub icon_tileset_uid: Option<i64>,
}
#[derive(Clone, Debug, Default)]
pub struct EnumSyncReport {
    pub applied: bool,
    pub requires_confirmation: bool,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}
fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn n(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn strings(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let sep = [',', ';', ' '].into_iter().find(|c| raw.contains(*c));
    sep.map(|sep| {
        raw.split(sep)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}
fn parsed(id: &str, values: Vec<String>) -> ParsedExternalEnum {
    ParsedExternalEnum {
        identifier: id.to_owned(),
        values: values
            .into_iter()
            .map(|id| json!({"id":id,"color":null,"tileRect":null}))
            .collect(),
        icon_tileset_uid: None,
    }
}
/// Parse source syntax without mutating a project. CastleDB icon records retain `_sourceTile` until import.
pub fn parse_external_enums(extension: &str, source: &str) -> Result<Vec<ParsedExternalEnum>> {
    let mut out = Vec::new();
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => {
            let v: Value =
                serde_json::from_str(source).map_err(|e| Error::Invalid(e.to_string()))?;
            if let Some(o) = v.as_object() {
                for (k, v) in o {
                    if let Some(a) = v.as_array() {
                        if a.first().is_some_and(Value::is_string) {
                            out.push(parsed(
                                k,
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect(),
                            ));
                        }
                    } else if let Some(v) = v.as_str() {
                        out.push(parsed(k, strings(v)));
                    }
                }
            }
        }
        "txt" => {
            for line in source.lines() {
                if let Some((id, values)) = line.split_once(':') {
                    out.push(parsed(
                        id.trim(),
                        strings(values.split(':').next().unwrap_or("")),
                    ));
                }
            }
        }
        "hx" => {
            let cleaned = strip_comments(source);
            let blocks = regex::Regex::new(r"(?im)^\s*enum\s+([a-z0-9_]+)\s*\{")
                .expect("constant Haxe enum expression");
            let values = regex::Regex::new(r"([A-Z][A-Za-z0-9_]*)[ \t]*;")
                .expect("constant Haxe value expression");
            for captures in blocks.captures_iter(&cleaned) {
                let start = captures.get(0).expect("complete capture").end();
                let mut depth = 1;
                let mut end = start;
                for (index, c) in cleaned[start..].char_indices() {
                    if c == '{' {
                        depth += 1;
                    } else if c == '}' {
                        depth -= 1;
                    }
                    if depth == 0 {
                        end = start + index;
                        break;
                    }
                }
                if depth != 0 {
                    continue;
                }
                let block = &cleaned[start..end];
                if !block.contains('(') {
                    let vals = values
                        .captures_iter(block)
                        .map(|c| c[1].to_owned())
                        .collect::<Vec<_>>();
                    if !vals.is_empty() {
                        out.push(parsed(&captures[1], vals));
                    }
                }
            }
        }
        "cdb" => {
            let v: Value =
                serde_json::from_str(source).map_err(|e| Error::Invalid(e.to_string()))?;
            for sheet in arr(&v["sheets"]) {
                let cols = arr(&sheet["columns"]);
                let Some(id) = cols
                    .iter()
                    .find(|c| s(c, "typeStr") == "0")
                    .map(|c| s(c, "name"))
                else {
                    continue;
                };
                let color = cols
                    .iter()
                    .find(|c| s(c, "typeStr") == "11")
                    .map(|c| s(c, "name"));
                let tile = sheet["props"]["displayIcon"].as_str();
                let mut values = Vec::new();
                let mut seen = BTreeSet::new();
                let name = s(sheet, "name");
                let lines: Vec<&Value> = if let Some((parent, rest)) = name.split_once('@') {
                    let list = rest.split('@').next().unwrap_or("");
                    let parent = arr(&v["sheets"])
                        .iter()
                        .find(|candidate| s(candidate, "name") == parent)
                        .ok_or_else(|| {
                            Error::Invalid(format!("CastleDB parent sheet missing for {name}"))
                        })?;
                    arr(&parent["lines"])
                        .iter()
                        .flat_map(|line| arr(&line[list]))
                        .collect()
                } else {
                    arr(&sheet["lines"]).iter().collect()
                };
                for line in lines {
                    let Some(value) = line[id].as_str() else {
                        continue;
                    };
                    if value.trim().is_empty() || !seen.insert(value) {
                        continue;
                    }
                    let mut value = json!({"id":value,"color":color.map(|c|line[c].clone()).unwrap_or(Value::Null),"tileRect":null});
                    if let Some(tile) = tile
                        && line[tile]["file"].is_string()
                    {
                        value["_sourceTile"] = line[tile].clone();
                    }
                    values.push(value);
                }
                out.push(ParsedExternalEnum {
                    identifier: s(sheet, "name").into(),
                    values,
                    icon_tileset_uid: None,
                });
            }
        }
        extension => {
            return Err(Error::Unsupported(format!(
                "external enum extension {extension}"
            )));
        }
    }
    Ok(out)
}
fn styled_identifier(raw: &str, style: &str) -> String {
    let mut id = clean_identifier(raw);
    match style {
        "Capitalize" => {
            if let Some(index) = id.bytes().position(|c| c != b'_')
                && id.as_bytes()[index].is_ascii_lowercase()
            {
                id.replace_range(index..index + 1, &id[index..index + 1].to_ascii_uppercase());
            }
        }
        "Uppercase" => id = id.to_ascii_uppercase(),
        "Lowercase" => id = id.to_ascii_lowercase(),
        _ => (),
    }
    id
}
fn strip_comments(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for c in chars.by_ref() {
                if c == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut previous = ' ';
            for c in chars.by_ref() {
                if previous == '*' && c == '/' {
                    break;
                }
                previous = c;
            }
        } else {
            out.push(c);
        }
    }
    out
}
fn enum_type(v: &Value, id: &str, uid: i64) -> bool {
    let public = v["__type"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches("Array<")
        .trim_end_matches('>');
    public == format!("ExternEnum.{id}")
        || public == format!("LocalEnum.{id}")
        || v["type"] == format!("F_Enum({uid})")
        || v["type"]["id"] == "F_Enum" && v["type"]["params"][0] == uid
}
fn contains(value: &Value, target: &str) -> bool {
    match value {
        Value::String(s) => s == target,
        Value::Array(a) => a.iter().any(|v| contains(v, target)),
        Value::Object(o) => o.get("params").is_some_and(|v| contains(v, target)),
        _ => false,
    }
}
fn used(v: &Value, id: &str, uid: i64, value: Option<&str>) -> bool {
    match v {
        Value::Object(o) => {
            (enum_type(v, id, uid)
                && value.is_none_or(|value| {
                    contains(&v["__value"], value) || contains(&v["defaultOverride"], value)
                }))
                || o.values().any(|v| used(v, id, uid, value))
        }
        Value::Array(a) => a.iter().any(|v| used(v, id, uid, value)),
        _ => false,
    }
}
fn replace_value(v: &mut Value, from: &str, to: Option<&str>) {
    match v {
        Value::String(s) if s == from => *v = to.map(|s| json!(s)).unwrap_or(Value::Null),
        Value::Array(a) => {
            for v in a {
                replace_value(v, from, to);
            }
        }
        Value::Object(o) => {
            if to.is_none() && o.get("params").is_some_and(|value| contains(value, from)) {
                *v = Value::Null;
            } else if let Some(v) = o.get_mut("params") {
                replace_value(v, from, to);
            }
        }
        _ => (),
    }
}
fn remap_field_values(v: &mut Value, id: &str, uid: i64, from: &str, to: Option<&str>) {
    if enum_type(v, id, uid) {
        for key in ["__value", "realEditorValues", "defaultOverride"] {
            if let Some(v) = v.get_mut(key) {
                replace_value(v, from, to);
            }
        }
    }
    match v {
        Value::Object(o) => {
            for v in o.values_mut() {
                remap_field_values(v, id, uid, from, to);
            }
        }
        Value::Array(a) => {
            for v in a {
                remap_field_values(v, id, uid, from, to);
            }
        }
        _ => (),
    }
}
fn rename_enum_types(v: &mut Value, from: &str, to: &str) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(s)) = o.get_mut("__type") {
                for prefix in ["LocalEnum", "ExternEnum"] {
                    if *s == format!("{prefix}.{from}") {
                        *s = format!("{prefix}.{to}");
                    } else if *s == format!("Array<{prefix}.{from}>") {
                        *s = format!("Array<{prefix}.{to}>");
                    }
                }
            }
            for v in o.values_mut() {
                rename_enum_types(v, from, to);
            }
        }
        Value::Array(a) => {
            for v in a {
                rename_enum_types(v, from, to);
            }
        }
        _ => (),
    }
}

impl Project {
    /// Change only local presentation overrides for an imported enum.
    pub fn update_external_enum_appearance(
        &mut self,
        uid: i64,
        atlas: Option<i64>,
        values: &[Value],
    ) -> Result<()> {
        self.edit_document(|p| {
            let definition = p.json["defs"]["externalEnums"]
                .as_array_mut()
                .and_then(|defs| defs.iter_mut().find(|def| def["uid"] == uid))
                .ok_or_else(|| Error::Invalid("external enum missing".into()))?;
            if Path::new(s(definition, "externalRelPath"))
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("cdb"))
            {
                return Err(Error::Invalid("CastleDB controls enum appearance".into()));
            }
            let stored = definition["values"]
                .as_array_mut()
                .ok_or_else(|| Error::Invalid("external enum values missing".into()))?;
            if stored.len() != values.len()
                || stored
                    .iter()
                    .zip(values)
                    .any(|(old, new)| old["id"] != new["id"])
            {
                return Err(Error::Invalid(
                    "external enum value IDs and order are read-only".into(),
                ));
            }
            for (old, value) in stored.iter_mut().zip(values) {
                for key in ["color", "tileRect"] {
                    if let Some(next) = value.get(key) {
                        old[key] = next.clone();
                    }
                }
            }
            definition["iconTilesetUid"] = json!(atlas);
            Ok(())
        })
    }

    pub fn external_enum_source_changed(&self, path: &Path) -> Result<bool> {
        let base = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."));
        let absolute = self.make_absolute_file_path(path)?;
        let relative = relative_to(&absolute, base)?;
        let source = fs::read(&absolute).map_err(|source| Error::Io {
            path: absolute,
            source,
        })?;
        let checksum = md5_hex(&source);
        Ok(arr(&self.json["defs"]["externalEnums"]).iter().any(|def| {
            s(def, "externalRelPath") == relative && s(def, "externalFileChecksum") != checksum
        }))
    }

    pub fn sync_external_enums(&mut self, path: impl AsRef<Path>) -> Result<EnumSyncReport> {
        self.apply_external_enum_sync(path, &BTreeMap::new(), false)
    }
    pub fn apply_external_enum_sync(
        &mut self,
        path: impl AsRef<Path>,
        renames: &BTreeMap<String, String>,
        allow_removals: bool,
    ) -> Result<EnumSyncReport> {
        let base = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."));
        let path = absolute(&base.join(path.as_ref()))?;
        let source = fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if source.is_empty() {
            return Err(Error::Invalid("external enum source is empty".into()));
        }
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let rel = relative_to(&path, base)?;
        let mut parsed = parse_external_enums(ext, &source)?;
        if parsed.is_empty() {
            return Err(Error::Invalid("source contains no supported enums".into()));
        }
        let mut candidate = self.clone();
        for kind in [
            "layers",
            "entities",
            "tilesets",
            "enums",
            "externalEnums",
            "levelFields",
        ] {
            if candidate.json["defs"][kind].is_null() {
                candidate.json["defs"][kind] = json!([]);
            } else if !candidate.json["defs"][kind].is_array() {
                return Err(Error::Invalid(format!("defs.{kind} must be an array")));
            }
        }
        if candidate.json["identifierStyle"].is_null() {
            candidate.json["identifierStyle"] = json!("Capitalize");
        }
        let mut ids = BTreeSet::new();
        let style = s(&candidate.json, "identifierStyle");
        for e in &mut parsed {
            e.identifier = styled_identifier(&e.identifier, style);
            if !ids.insert(e.identifier.clone()) {
                return Err(Error::Invalid("duplicate source enum identifier".into()));
            }
            let mut seen = BTreeSet::new();
            for value in &mut e.values {
                let mut base = styled_identifier(s(value, "id"), style);
                if base == "_" {
                    base = "Unnamed".into();
                }
                let mut id = base.clone();
                if seen.contains(&id) {
                    let suffix = base.bytes().rev().take_while(u8::is_ascii_digit).count();
                    let (prefix, mut index) = if suffix > 0 {
                        (
                            &base[..base.len() - suffix],
                            base[base.len() - suffix..].parse::<u64>().unwrap_or(2),
                        )
                    } else {
                        (base.as_str(), 2)
                    };
                    while seen.contains(&id) {
                        id = format!("{prefix}{index}");
                        index += 1;
                    }
                }
                seen.insert(id.clone());
                value["id"] = json!(id);
            }
        }
        // Preserve UID on source enum rename, and propagate cached field type labels.
        for (from, to) in renames.iter().filter(|(k, _)| !k.contains('.')) {
            if !ids.contains(to) {
                return Err(Error::Invalid(format!(
                    "rename target enum {to} not in source"
                )));
            }
            if from != to
                && arr(&candidate.json["defs"]["externalEnums"])
                    .iter()
                    .any(|e| s(e, "identifier") == to)
            {
                return Err(Error::Invalid(format!(
                    "rename target enum {to} already exists; enum definitions cannot be merged"
                )));
            }
            if let Some(e) = candidate.json["defs"]["externalEnums"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|e| s(e, "identifier") == from && s(e, "externalRelPath") == rel)
            {
                e["identifier"] = json!(to);
            }
            rename_enum_types(&mut candidate.json, from, to);
            for level in candidate.external_levels.values_mut() {
                rename_enum_types(level, from, to);
            }
        }
        for e in &parsed {
            for kind in ["enums", "externalEnums"] {
                if arr(&candidate.json["defs"][kind]).iter().any(|existing| {
                    s(existing, "identifier") == e.identifier
                        && s(existing, "externalRelPath") != rel
                }) {
                    return Err(Error::Invalid(format!(
                        "enum {} conflicts with an existing definition",
                        e.identifier
                    )));
                }
            }
        }
        let old: Vec<_> = arr(&candidate.json["defs"]["externalEnums"])
            .iter()
            .filter(|e| s(e, "externalRelPath") == rel)
            .cloned()
            .collect();
        let mut report = EnumSyncReport::default();
        let mut removals = Vec::new();
        for e in &old {
            let id = s(e, "identifier");
            let enum_uid = n(e, "uid");
            let new = parsed.iter().find(|p| p.identifier == id);
            if new.is_none() {
                report.removed.push(id.into());
                let is_used = used(&candidate.json, id, enum_uid, None)
                    || candidate
                        .external_levels
                        .values()
                        .any(|v| used(v, id, enum_uid, None));
                if is_used && !allow_removals {
                    report.requires_confirmation = true;
                }
            }
            for value in arr(&e["values"]) {
                let value_id = s(value, "id");
                if new.is_some_and(|e| e.values.iter().any(|v| s(v, "id") == value_id)) {
                    continue;
                }
                let rename = renames.get(&format!("{id}.{value_id}")).or_else(|| {
                    renames
                        .iter()
                        .find(|(from, to)| !from.contains('.') && to.as_str() == id)
                        .and_then(|(from, _)| renames.get(&format!("{from}.{value_id}")))
                });
                if let Some(to) = rename
                    && !new.is_some_and(|e| e.values.iter().any(|v| s(v, "id") == to))
                {
                    return Err(Error::Invalid(format!(
                        "rename target {id}.{to} not in source"
                    )));
                }
                let in_use = used(&candidate.json, id, enum_uid, Some(value_id))
                    || candidate
                        .external_levels
                        .values()
                        .any(|v| used(v, id, enum_uid, Some(value_id)));
                if in_use && rename.is_none() && !allow_removals {
                    report.requires_confirmation = true;
                }
                report.removed.push(format!("{id}.{value_id}"));
                removals.push((
                    id.to_owned(),
                    enum_uid,
                    value_id.to_owned(),
                    rename.cloned(),
                ));
            }
        }
        if report.requires_confirmation {
            return Ok(report);
        }
        for (id, enum_uid, from, to) in removals {
            remap_field_values(&mut candidate.json, &id, enum_uid, &from, to.as_deref());
            for level in candidate.external_levels.values_mut() {
                remap_field_values(level, &id, enum_uid, &from, to.as_deref());
            }
            let biome_fields: BTreeSet<_> = arr(&candidate.json["defs"]["levelFields"])
                .iter()
                .filter(|f| enum_type(f, &id, enum_uid))
                .map(|f| n(f, "uid"))
                .collect();
            for layer in candidate.json["defs"]["layers"].as_array_mut().unwrap() {
                if layer["biomeFieldUid"]
                    .as_i64()
                    .is_some_and(|f| biome_fields.contains(&f))
                {
                    for group in layer["autoRuleGroups"].as_array_mut().into_iter().flatten() {
                        replace_value(&mut group["requiredBiomeValues"], &from, to.as_deref());
                        if let Some(values) = group["requiredBiomeValues"].as_array_mut() {
                            values.retain(|v| !v.is_null());
                        }
                    }
                }
            }
            for tileset in candidate.json["defs"]["tilesets"].as_array_mut().unwrap() {
                if tileset["tagsSourceEnumUid"] == enum_uid {
                    let mut tags = Vec::<Value>::new();
                    for mut tag in arr(&tileset["enumTags"]).to_vec() {
                        if tag["enumValueId"] == from {
                            if let Some(to) = &to {
                                tag["enumValueId"] = json!(to);
                            } else {
                                continue;
                            }
                        }
                        if let Some(existing) = tags
                            .iter_mut()
                            .find(|entry| entry["enumValueId"] == tag["enumValueId"])
                        {
                            for tile in arr(&tag["tileIds"]) {
                                if !arr(&existing["tileIds"]).contains(tile) {
                                    existing["tileIds"]
                                        .as_array_mut()
                                        .unwrap()
                                        .push(tile.clone());
                                }
                            }
                        } else {
                            tags.push(tag);
                        }
                    }
                    tileset["enumTags"] = json!(tags);
                }
            }
        }
        let checksum = md5_hex(source.as_bytes());
        let mut replacements = Vec::new();
        for mut e in parsed {
            let existing = old.iter().find(|old| s(old, "identifier") == e.identifier);
            let enum_uid = if let Some(existing) = existing {
                n(existing, "uid")
            } else {
                let id = crate::workflow::alloc_uid(&mut candidate)?;
                report.added.push(e.identifier.clone());
                id
            };
            if ext.eq_ignore_ascii_case("cdb") {
                for value in &mut e.values {
                    if let Some(tile) = value.get("_sourceTile").cloned() {
                        let atlas = path.parent().unwrap().join(s(&tile, "file"));
                        let rel_atlas = relative_to(&atlas, base)?;
                        let existing = arr(&candidate.json["defs"]["tilesets"])
                            .iter()
                            .find(|td| s(td, "relPath") == rel_atlas)
                            .map(|td| n(td, "uid"));
                        let tileset_uid = if let Some(id) = e.icon_tileset_uid.or(existing) {
                            id
                        } else {
                            let image = crate::exports::load_image_rgba(&atlas)?;
                            let occupied = arr(&candidate.json["defs"]["tilesets"])
                                .iter()
                                .map(|def| s(def, "identifier").to_owned())
                                .collect();
                            let name = format!(
                                "{}_{}",
                                path.file_name().unwrap_or_default().to_string_lossy(),
                                atlas.file_name().unwrap_or_default().to_string_lossy()
                            );
                            let identifier = crate::naming::unique_name(
                                &name,
                                s(&candidate.json, "identifierStyle"),
                                &occupied,
                            );
                            // The icon size controls its crop, while LDtk creates a standard tileset.
                            let id = candidate.add_tileset(
                                Path::new(&rel_atlas),
                                16,
                                image.width() as i32,
                                image.height() as i32,
                            )?;
                            let definition = candidate.json["defs"]["tilesets"]
                                .as_array_mut()
                                .unwrap()
                                .last_mut()
                                .expect("added tileset");
                            definition["identifier"] = json!(identifier);
                            definition["tags"] = json!(["CastleDB"]);
                            id
                        };
                        e.icon_tileset_uid = Some(tileset_uid);
                        let size = n(&tile, "size");
                        value["tileRect"] = json!({"tilesetUid":tileset_uid,"x":n(&tile,"x")*size,"y":n(&tile,"y")*size,"w":tile["width"].as_i64().unwrap_or(1)*size,"h":tile["height"].as_i64().unwrap_or(1)*size});
                        value.as_object_mut().unwrap().remove("_sourceTile");
                    }
                }
            }
            for v in &mut e.values {
                let old_value = existing
                    .and_then(|existing| {
                        arr(&existing["values"])
                            .iter()
                            .find(|old| old["id"] == v["id"])
                    })
                    .or_else(|| {
                        let old_name = renames
                            .iter()
                            .find(|(from, to)| {
                                from.contains('.')
                                    && to.as_str() == s(v, "id")
                                    && from.split_once('.').is_some_and(|(enu, _)| {
                                        enu == e.identifier
                                            || renames
                                                .get(enu)
                                                .is_some_and(|to| *to == e.identifier)
                                    })
                            })
                            .and_then(|(from, _)| from.split_once('.').map(|(_, value)| value));
                        existing.and_then(|existing| {
                            arr(&existing["values"])
                                .iter()
                                .find(|old| Some(s(old, "id")) == old_name)
                        })
                    });
                if let Some(old) = old_value {
                    if !ext.eq_ignore_ascii_case("cdb") {
                        let id = v["id"].clone();
                        *v = old.clone();
                        v["id"] = id;
                    }
                } else {
                    if v["color"].is_null() {
                        v["color"] = json!(0);
                    }
                    report
                        .added
                        .push(format!("{}.{}", e.identifier, s(v, "id")));
                }
            }
            let mut value = existing
                .cloned()
                .unwrap_or_else(|| json!({"uid":enum_uid,"identifier":e.identifier,"tags":[]}));
            value["identifier"] = json!(e.identifier);
            value["externalRelPath"] = json!(rel);
            value["externalFileChecksum"] = json!(checksum);
            value["iconTilesetUid"] = if ext.eq_ignore_ascii_case("cdb") {
                json!(e.icon_tileset_uid)
            } else {
                existing
                    .map(|e| e["iconTilesetUid"].clone())
                    .unwrap_or(Value::Null)
            };
            value["values"] = json!(e.values);
            if existing.is_some_and(|old| old != &value) {
                report.changed.push(e.identifier);
            }
            replacements.push(value);
        }
        let removed_enums: Vec<_> = old
            .iter()
            .filter(|e| report.removed.contains(&s(e, "identifier").to_owned()))
            .collect();
        for tileset in candidate.json["defs"]["tilesets"].as_array_mut().unwrap() {
            if removed_enums
                .iter()
                .any(|e| tileset["tagsSourceEnumUid"] == e["uid"])
            {
                tileset["tagsSourceEnumUid"] = Value::Null;
                tileset["enumTags"] = json!([]);
            }
        }
        let removed_fields: Vec<_> = arr(&candidate.json["defs"]["levelFields"])
            .iter()
            .chain(
                arr(&candidate.json["defs"]["entities"])
                    .iter()
                    .flat_map(|e| arr(&e["fieldDefs"])),
            )
            .filter(|field| {
                removed_enums
                    .iter()
                    .any(|e| enum_type(field, s(e, "identifier"), n(e, "uid")))
            })
            .map(|f| n(f, "uid"))
            .collect();
        for uid in removed_fields {
            candidate.remove_field_definition(uid)?;
        }
        let enums = candidate.json["defs"]["externalEnums"]
            .as_array_mut()
            .ok_or_else(|| Error::Invalid("externalEnums missing".into()))?;
        enums.retain(|e| s(e, "externalRelPath") != rel);
        enums.extend(replacements);
        candidate.refresh_derived_data()?;
        candidate
            .external_levels
            .reuse_equal_documents(&self.external_levels);
        *self = candidate;
        report.applied = true;
        Ok(report)
    }
}
fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let mut raw = data.to_vec();
    let bit_len = (raw.len() as u64) * 8;
    raw.push(0x80);
    while raw.len() % 64 != 56 {
        raw.push(0);
    }
    raw.extend_from_slice(&bit_len.to_le_bytes());
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    for block in raw.as_chunks::<64>().0 {
        let mut words = [0u32; 16];
        for (i, w) in block.as_chunks::<4>().0.iter().enumerate() {
            words[i] = u32::from_le_bytes(*w);
        }
        let [a0, b0, c0, d0] = state;
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for (i, shift) in S.iter().enumerate() {
            let (f, g) = if i < 16 {
                ((b & c) | (!b & d), i)
            } else if i < 32 {
                ((d & b) | (!d & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * i) % 16)
            };
            let k = ((i as f64 + 1.).sin().abs() * 4294967296.).floor() as u32;
            let v = a
                .wrapping_add(f)
                .wrapping_add(k)
                .wrapping_add(words[g])
                .rotate_left(*shift);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(v);
        }
        state = [
            a0.wrapping_add(a),
            b0.wrapping_add(b),
            c0.wrapping_add(c),
            d0.wrapping_add(d),
        ];
    }
    state
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl Project {
    /// Stop importing one source file and remove its dependent enum fields, without deleting the file.
    pub fn remove_external_enum_source(&mut self, path: &str) -> Result<usize> {
        self.edit_document(|p| {
            let ids = p.json["defs"]["externalEnums"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["externalRelPath"] == path)
                .filter_map(|d| d["uid"].as_i64())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return Err(Error::Invalid("external enum source not found".into()));
            }
            for uid in &ids {
                p.remove_definition("externalEnums", *uid)?;
            }
            Ok(ids.len())
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn standard_md5_vectors() {
        assert_eq!(super::md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(super::md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }
}
