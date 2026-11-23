//! Explicit editor migration; raw project loading deliberately remains lossless.
use crate::grid_presence::GridPresence;
use crate::workflow::{layer_cell_extent, tidy_gridvania_defaults};
use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn n(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
fn array(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn defaults(v: &mut Value, template: &Value) {
    if let (Some(v), Some(t)) = (v.as_object_mut(), template.as_object()) {
        for (k, value) in t {
            if !v.contains_key(k) {
                v.insert(k.clone(), value.clone());
            }
        }
    }
}
fn default_world_dimensions(world: &mut Value, template: &Value) {
    for (default_size, grid_step) in [
        ("defaultLevelWidth", "worldGridWidth"),
        ("defaultLevelHeight", "worldGridHeight"),
    ] {
        if world[default_size].is_null() {
            world[default_size] = template[default_size].clone();
        }
        if world[grid_step].is_null() {
            world[grid_step] = world[default_size].clone();
        }
    }
    tidy_gridvania_defaults(world);
}
fn alias(v: &mut Value, old: &str, new: &str) {
    if let Some(v) = v.as_object_mut()
        && let Some(value) = v.remove(old)
        && !value.is_null()
    {
        v.insert(new.into(), value);
    }
}
fn uuid(v: &mut Value) {
    if !v["iid"].is_string() {
        v["iid"] = json!(uuid::Uuid::new_v4().to_string());
    }
}
fn before(version: &str, major: u32, minor: u32) -> bool {
    let mut parts = version.split('.').map(|s| s.parse::<u32>().unwrap_or(0));
    (parts.next().unwrap_or(1), parts.next().unwrap_or(5)) < (major, minor)
}
fn rect(td: &Value, id: i64) -> Value {
    let grid = n(td, "tileGridSize").max(1);
    let spacing = n(td, "spacing");
    let pad = n(td, "padding");
    let cols = ((n(td, "pxWid") - 2 * pad + spacing) / (grid + spacing)).max(1);
    json!({"tilesetUid":td["uid"],"x":pad+id%cols*(grid+spacing),"y":pad+id/cols*(grid+spacing),"w":grid,"h":grid})
}
fn field_type(field: &mut Value, enums: &[(i64, String, bool)]) {
    alias(field, "name", "identifier");
    alias(field, "textLangageMode", "textLanguageMode");
    if let Some(uid) = field["type"]
        .as_str()
        .and_then(|s| s.strip_prefix("F_Enum("))
        .and_then(|s| s.strip_suffix(')'))
        .and_then(|s| s.parse::<i64>().ok())
    {
        field["type"] = json!({"id":"F_Enum","params":[uid]});
    }
    if field["type"] == "F_File" {
        field["type"] = json!("F_Path");
    }
    if field["editorDisplayMode"] == "RefLink" {
        field["editorDisplayMode"] = json!("RefLinkBetweenCenters");
    }
    if field["regex"] == "//g" {
        field["regex"] = Value::Null;
    }
    let typ = match s(field, "type") {
        "F_Int" => "Int",
        "F_Float" => "Float",
        "F_Bool" => "Bool",
        "F_Color" => "Color",
        "F_String" => "String",
        "F_Text" => "Multilines",
        "F_Path" => "FilePath",
        "F_Point" => "Point",
        "F_Tile" => "Tile",
        "F_EntityRef" => "EntityRef",
        _ => "",
    };
    let typ = if typ.is_empty() {
        let uid = field["type"]["params"][0].as_i64().unwrap_or(-1);
        enums
            .iter()
            .find(|e| e.0 == uid)
            .map(|e| format!("{}Enum.{}", if e.2 { "Extern" } else { "Local" }, e.1))
            .unwrap_or_else(|| {
                s(field, "__type")
                    .trim_start_matches("Array<")
                    .trim_end_matches('>')
                    .to_owned()
            })
    } else {
        typ.to_owned()
    };
    if field["type"]["id"] == "F_Enum" {
        field["type"] = json!(format!(
            "F_Enum({})",
            field["type"]["params"][0].as_i64().unwrap_or(-1)
        ));
    }
    field["__type"] = json!(if field["isArray"] == true {
        format!("Array<{typ}>")
    } else {
        typ
    });
    defaults(
        field,
        &json!({"isArray":false,"canBeNull":false,"defaultOverride":null,"arrayMinLength":null,"arrayMaxLength":null,"min":null,"max":null,"regex":null,"acceptFileTypes":null,"doc":null,"editorDisplayMode":"Hidden","editorDisplayPos":"Above","editorDisplayScale":1,"editorLinkStyle":"StraightArrow","editorAlwaysShow":false,"editorShowInWorld":true,"editorCutLongValues":true,"editorTextPrefix":null,"editorTextSuffix":null,"useForSmartColor":false,"textLanguageMode":null,"symmetricalRef":false,"autoChainRef":true,"allowOutOfLevelRef":true,"allowedRefs":"OnlySame","allowedRefsEntityUid":null,"allowedRefTags":[],"tilesetUid":null,"exportToToc":false,"searchable":false}),
    );
}
fn field_instance(fi: &mut Value, defs: &BTreeMap<i64, Value>) {
    if fi["realEditorValues"].is_null() {
        let raw = fi
            .get_mut("realEditorValue")
            .map(Value::take)
            .unwrap_or(Value::Null);
        fi["realEditorValues"] = json!([raw]);
    }
    let Some(d) = defs.get(&n(fi, "defUid")) else {
        return;
    };
    fi["__identifier"] = d["identifier"].clone();
    fi["__type"] = d["__type"].clone();
    if fi.get("__value").is_none() {
        let typ = s(d, "__type")
            .trim_start_matches("Array<")
            .trim_end_matches('>');
        let wrappers = array(&fi["realEditorValues"]);
        let values: Vec<_> = wrappers
            .iter()
            .map(|w| {
                let raw = if w.is_null() {
                    d["defaultOverride"]["params"][0].clone()
                } else {
                    w["params"][0].clone()
                };
                match typ {
                    "Color" => raw
                        .as_i64()
                        .map(|i| json!(format!("#{:06X}", i & 0xffffff)))
                        .unwrap_or(Value::Null),
                    "Point" => raw
                        .as_str()
                        .and_then(|s| s.split_once(','))
                        .and_then(|(x, y)| {
                            Some(json!({"cx":x.parse::<i64>().ok()?,"cy":y.parse::<i64>().ok()?}))
                        })
                        .unwrap_or(Value::Null),
                    _ => raw,
                }
            })
            .collect();
        fi["__value"] = if d["isArray"] == true {
            json!(values)
        } else {
            values.first().cloned().unwrap_or(Value::Null)
        };
    }
    if fi.get("__tile").is_none() {
        fi["__tile"] = Value::Null;
    }
}
fn reconcile_field_instances(
    instances: &mut Value,
    owner_defs: Option<&[Value]>,
    field_defs: &BTreeMap<i64, Value>,
) -> Result<()> {
    if instances.is_null() {
        *instances = json!([]);
    }
    let fields = instances
        .as_array_mut()
        .ok_or_else(|| Error::Invalid("field instances must be an array".into()))?;
    if fields.iter().any(|field| !field.is_object()) {
        return Err(Error::Invalid("field instance must be an object".into()));
    }
    let mut present = owner_defs.map(|defs| {
        defs.iter()
            .filter_map(|def| Some((def["uid"].as_i64()?, false)))
            .collect::<BTreeMap<_, _>>()
    });
    fields.retain_mut(|field| {
        alias(field, "defId", "defUid");
        if let Some(present) = &mut present
            && let Some(uid) = field["defUid"].as_i64()
        {
            let Some(present) = present.get_mut(&uid) else {
                return false;
            };
            *present = true;
        }
        field_instance(field, field_defs);
        true
    });
    for (uid, present) in present.into_iter().flatten() {
        if !present {
            // Leave overrides empty so future edits to the definition's default still apply.
            fields.push(json!({"defUid":uid,"realEditorValues":[]}));
        }
    }
    Ok(())
}
impl Project {
    /// Apply the source loader's historical aliases, missing defaults, UUIDs and pre-CSV/tile migrations.
    /// Unknown properties remain intact; callers choose whether to persist this upgraded document.
    pub fn migrate_to_current(&mut self) -> Result<bool> {
        let version = s(&self.json, "jsonVersion");
        let parts = version
            .split('.')
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>();
        if let Ok(parts) = parts
            && parts.len() == 3
            && (parts[0], parts[1], parts[2]) > (1, 5, 4)
        {
            return Err(Error::Invalid(format!(
                "Project JSON version {version} is newer than the supported 1.5.4"
            )));
        }
        let mut p = self.clone();
        let version = s(&p.json, "jsonVersion").to_owned();
        let template = Project::new();
        alias(&mut p.json, "advancedOptionFlags", "flags");
        if !p.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.iter().any(|flag| flag == "MultiWorlds"))
        {
            default_world_dimensions(&mut p.json, &template.json);
        }
        if let Some(export) = p.json.as_object_mut().unwrap().remove("exportPng")
            && !export.is_null()
        {
            p.json["imageExportMode"] = json!(if export == true {
                "OneImagePerLayer"
            } else {
                "None"
            });
        }
        if p.json.get("jsonStyle").is_none() {
            p.json["jsonStyle"] = json!("Compact");
        }
        if !p.json["defaultLevelBgColor"].is_string() {
            p.json["defaultLevelBgColor"] = p
                .json
                .get("bgColor")
                .filter(|color| color.is_string())
                .unwrap_or(&template.json["bgColor"])
                .clone();
        }
        defaults(&mut p.json, &template.json);
        uuid(&mut p.json);
        if !p.json["dummyWorldIid"].is_string() {
            p.json["dummyWorldIid"] = json!(uuid::Uuid::new_v4().to_string());
        }
        for name in [
            "layers",
            "entities",
            "tilesets",
            "enums",
            "externalEnums",
            "levelFields",
        ] {
            if p.json["defs"][name].is_null() {
                p.json["defs"][name] = json!([]);
            }
        }
        if before(&version, 1, 5) {
            let flags = p.json["flags"]
                .as_array_mut()
                .ok_or_else(|| Error::Invalid("flags must be an array".into()))?;
            if !flags.contains(&json!("ExportOldTableOfContentData")) {
                flags.push(json!("ExportOldTableOfContentData"));
            }
        }
        if before(&version, 0, 10) {
            let flags = p.json["flags"].as_array_mut().unwrap();
            if !flags.contains(&json!("PrependIndexToLevelFileNames")) {
                flags.push(json!("PrependIndexToLevelFileNames"));
            }
        }
        if let Some(flags) = p.json["flags"].as_array_mut() {
            flags.retain(|v| v != "DiscardPreCsvIntGrid");
        }
        if before(&version, 0, 6) && self.json.get("worldLayout").is_none() {
            p.json["worldLayout"] = json!("LinearHorizontal");
        }
        // Default records are the same complete definitions used by native creation.
        let mut factory = Project::new();
        factory.add_layer("Layer", "IntGrid", 16)?;
        factory.add_entity_def("Entity", 16, 16)?;
        let layer_template = factory.json["defs"]["layers"][0].clone();
        let entity_template = factory.json["defs"]["entities"][0].clone();
        for td in p.json["defs"]["tilesets"].as_array_mut().unwrap() {
            alias(td, "path", "relPath");
            alias(td, "metaDataEnumUid", "tagsSourceEnumUid");
            alias(td, "metaDataEnumValues", "enumTags");
            defaults(
                td,
                &json!({"identifier":format!("Tileset{}",n(td,"uid")),"tileGridSize":16,"spacing":0,"padding":0,"relPath":null,"embedAtlas":null,"tags":[],"enumTags":[],"customData":[],"savedSelections":[],"cachedPixelData":null,"tagsSourceEnumUid":null}),
            );
            let grid = n(td, "tileGridSize").max(1);
            let spacing = n(td, "spacing");
            let padding = n(td, "padding");
            td["__cWid"] = json!((n(td, "pxWid") - 2 * padding + spacing) / (grid + spacing));
            td["__cHei"] = json!((n(td, "pxHei") - 2 * padding + spacing) / (grid + spacing));
        }
        let tilesets: BTreeMap<_, _> = array(&p.json["defs"]["tilesets"])
            .iter()
            .map(|t| (n(t, "uid"), t.clone()))
            .collect();
        let layer_uids: BTreeSet<_> = array(&p.json["defs"]["layers"])
            .iter()
            .filter_map(|layer| layer["uid"].as_i64())
            .collect();
        for d in p.json["defs"]["layers"].as_array_mut().unwrap() {
            if d["layerUidsPreventingAutoTilingHere"].is_null() {
                d["layerUidsPreventingAutoTilingHere"] =
                    match d["autoTilesKilledByOtherLayerUid"].as_i64() {
                        Some(uid) => json!([uid]),
                        None => json!([]),
                    };
            }
            d.as_object_mut()
                .unwrap()
                .remove("autoTilesKilledByOtherLayerUid");
            alias(d, "tilesetDefId", "tilesetDefUid");
            if d["tilesetDefUid"].is_null() {
                alias(d, "autoTilesetDefUid", "tilesetDefUid");
            }
            if d["inactiveOpacity"].is_null() {
                if d["fadeInactive"] == true {
                    d["inactiveOpacity"] = json!(0.2);
                } else if d["type"] == "Entities" {
                    d["inactiveOpacity"] = json!(0.6);
                }
            }
            if let Some(f) = d.as_object_mut().unwrap().remove("parallaxFactor") {
                d["parallaxFactorX"] = f.clone();
                d["parallaxFactorY"] = f;
            }
            let id = d["uid"].clone();
            let name = d["identifier"].clone();
            let kind = d["type"].clone();
            if d.get("hideFieldsWhenInactive").is_none() {
                d["hideFieldsWhenInactive"] = json!(true);
            }
            if kind != "IntGrid" && d.get("intGridValues").is_none() {
                d["intGridValues"] = json!([]);
            }
            defaults(d, &layer_template);
            if kind == "AutoLayer"
                && d["autoSourceLayerDefUid"]
                    .as_i64()
                    .is_some_and(|uid| !layer_uids.contains(&uid))
            {
                d["autoSourceLayerDefUid"] = Value::Null;
            }
            d["uid"] = id;
            d["identifier"] = name;
            d["type"] = kind.clone();
            d["__type"] = kind;
            for (i, v) in d["intGridValues"]
                .as_array_mut()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if !v["value"].is_number() {
                    v["value"] = json!(i + 1);
                }
                defaults(v, &json!({"identifier":null,"tile":null,"groupUid":0}));
            }
            for group in d["autoRuleGroups"].as_array_mut().into_iter().flatten() {
                defaults(
                    group,
                    &json!({"active":true,"isOptional":false,"usesWizard":false,"requiredBiomeValues":[],"biomeRequirementMode":0,"color":null,"icon":null}),
                );
                for rule in group["rules"].as_array_mut().into_iter().flatten() {
                    if let Some(ids) = rule.as_object_mut().unwrap().remove("tileIds") {
                        rule["tileRectsIds"] = if rule["tileMode"] == "Stamp" {
                            json!([ids])
                        } else {
                            json!(array(&ids).iter().map(|v| json!([v])).collect::<Vec<_>>())
                        };
                    }
                    defaults(
                        rule,
                        &json!({"active":true,"breakOnMatch":false,"alpha":1,"outOfBoundsValue":null,"flipX":false,"flipY":false,"checker":"None","tileMode":"Single","pivotX":0,"pivotY":0,"xModulo":1,"yModulo":1,"xOffset":0,"yOffset":0,"tileXOffset":0,"tileYOffset":0,"tileRandomXMin":0,"tileRandomXMax":0,"tileRandomYMin":0,"tileRandomYMax":0,"invalidated":false,"perlinActive":false,"perlinScale":0.2,"perlinOctaves":2,"perlinSeed":0}),
                    );
                }
            }
        }
        for kind in ["enums", "externalEnums"] {
            for e in p.json["defs"][kind].as_array_mut().unwrap() {
                defaults(
                    e,
                    &json!({"iconTilesetUid":null,"externalRelPath":null,"externalFileChecksum":null,"tags":[],"values":[]}),
                );
                let ts = e["iconTilesetUid"]
                    .as_i64()
                    .and_then(|id| tilesets.get(&id));
                for v in e["values"].as_array_mut().unwrap() {
                    if v["tileRect"].is_null()
                        && let (Some(td), Some(id)) = (ts, v["tileId"].as_i64())
                        && id >= 0
                    {
                        v["tileRect"] = rect(td, id);
                    }
                    defaults(v, &json!({"tileRect":null,"color":0}));
                }
            }
        }
        let enums: Vec<_> = [("enums", false), ("externalEnums", true)]
            .into_iter()
            .flat_map(|(kind, ext)| {
                array(&p.json["defs"][kind])
                    .iter()
                    .map(move |e| (n(e, "uid"), s(e, "identifier").to_owned(), ext))
            })
            .collect();
        for e in p.json["defs"]["entities"].as_array_mut().unwrap() {
            alias(e, "name", "identifier");
            alias(e, "maxPerLevel", "maxCount");
            if e["tileOpacity"].is_null() {
                if e["hollow"] == true {
                    e["tileOpacity"] = json!(0.25);
                    e["fillOpacity"] = json!(0.15);
                } else if e["renderMode"] == "Tile" {
                    e["tileOpacity"] = e["fillOpacity"].clone();
                    e["fillOpacity"] = json!(0.08);
                    e["lineOpacity"] = json!(0);
                }
            }
            if e["tileRenderMode"] == "Crop" {
                e["tileRenderMode"] = json!("Cover");
            }
            if e["discardExcess"] == false {
                e["limitBehavior"] = json!("PreventAdding");
            }
            if e["tileRect"].is_null()
                && let (Some(td), Some(id)) = (
                    e["tilesetId"].as_i64().and_then(|id| tilesets.get(&id)),
                    e["tileId"].as_i64(),
                )
                && id >= 0
            {
                e["tileRect"] = rect(td, id);
            }
            if e["tileRect"].is_object() && e["tileRect"]["tilesetUid"].is_null() {
                e["tileRect"]["tilesetUid"] = e["tilesetId"].clone();
            }
            let uid = e["uid"].clone();
            let identifier = e["identifier"].clone();
            defaults(e, &json!({"pivotX":0,"pivotY":0,"color":"#000000"}));
            defaults(e, &entity_template);
            e["uid"] = uid;
            e["identifier"] = identifier;
            for f in e["fieldDefs"].as_array_mut().unwrap() {
                field_type(f, &enums);
            }
        }
        for f in p.json["defs"]["levelFields"].as_array_mut().unwrap() {
            field_type(f, &enums);
        }
        let layer_defs: BTreeMap<_, _> = array(&p.json["defs"]["layers"])
            .iter()
            .map(|d| (n(d, "uid"), d.clone()))
            .collect();
        let entity_defs: BTreeMap<_, _> = array(&p.json["defs"]["entities"])
            .iter()
            .map(|d| (n(d, "uid"), d.clone()))
            .collect();
        let level_field_defs = array(&p.json["defs"]["levelFields"]).to_vec();
        let field_defs: BTreeMap<_, _> = level_field_defs
            .iter()
            .chain(
                entity_defs
                    .values()
                    .flat_map(|e| array(&e["fieldDefs"]).iter()),
            )
            .map(|f| (n(f, "uid"), f.clone()))
            .collect();
        let bg = p.json["defaultLevelBgColor"].clone();
        let mut grid_presence = std::mem::take(&mut p.grid_presence);
        let mut positions: BTreeMap<Option<usize>, i64> = BTreeMap::new();
        let world_layout = s(&p.json, "worldLayout").to_owned();
        for key in p.level_keys() {
            let l = p
                .level_mut(key)
                .ok_or_else(|| Error::Invalid("external level not loaded".into()))?;
            uuid(l);
            let id = l["uid"].clone();
            let name = l["identifier"].clone();
            defaults(l, &template.json["levels"][0]);
            l["uid"] = id;
            l["identifier"] = name;
            if before(&version, 0, 6) {
                let pos = positions.entry(key.world).or_default();
                match world_layout.as_str() {
                    "LinearHorizontal" => {
                        l["worldX"] = json!(*pos);
                        l["worldY"] = json!(0);
                        *pos += n(l, "pxWid") + 48;
                    }
                    "LinearVertical" => {
                        l["worldX"] = json!(0);
                        l["worldY"] = json!(*pos);
                        *pos += n(l, "pxHei") + 48;
                    }
                    _ => (),
                }
            }
            if l["__bgColor"].is_null() {
                l["__bgColor"] = bg.clone();
            }
            reconcile_field_instances(
                &mut l["fieldInstances"],
                Some(&level_field_defs),
                &field_defs,
            )?;
            let level_uid = l["uid"].clone();
            let (w, h) = (n(l, "pxWid"), n(l, "pxHei"));
            for li in l["layerInstances"].as_array_mut().ok_or_else(|| {
                Error::Invalid("level layers must be loaded before migration".into())
            })? {
                alias(li, "layerDefId", "layerDefUid");
                uuid(li);
                let d = layer_defs
                    .get(&n(li, "layerDefUid"))
                    .ok_or_else(|| Error::Invalid("legacy layer definition missing".into()))?;
                let grid = n(d, "gridSize").max(1);
                let ox = n(li, "pxOffsetX");
                let oy = n(li, "pxOffsetY");
                let extent = (
                    layer_cell_extent(w, ox, grid),
                    layer_cell_extent(h, oy, grid),
                );
                let legacy = (layer_cell_extent(w, 0, grid), layer_cell_extent(h, 0, grid));
                let (cw, ch) = if legacy != extent
                    && n(li, "__gridSize") == grid
                    && (n(li, "__cWid"), n(li, "__cHei")) == legacy
                    && (d["type"] != "IntGrid"
                        || li["intGridCsv"].as_array().is_some_and(|cells| {
                            legacy.0.checked_mul(legacy.1) == i64::try_from(cells.len()).ok()
                        })) {
                    // Older LevelEditor grid-size edits saved a stride ignoring the offset.
                    legacy
                } else {
                    extent
                };
                li["__identifier"] = d["identifier"].clone();
                li["__type"] = d["type"].clone();
                li["__cWid"] = json!(cw);
                li["__cHei"] = json!(ch);
                li["__gridSize"] = json!(grid);
                li["levelId"] = level_uid.clone();
                defaults(
                    li,
                    &json!({"pxOffsetX":0,"pxOffsetY":0,"visible":true,"optionalRules":[],"seed":0,"overrideTilesetUid":null,"autoLayerTiles":[],"gridTiles":[],"entityInstances":[]}),
                );
                if li["intGridCsv"].is_null() {
                    let mut presence = GridPresence::empty();
                    let count = if d["type"] == "IntGrid" {
                        cw.checked_mul(ch)
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or_else(|| {
                                Error::Invalid("legacy IntGrid dimensions overflow".into())
                            })?
                    } else {
                        0
                    };
                    let mut csv = vec![0; count];
                    for cell in array(&li["intGrid"]) {
                        let coord = n(cell, "coordId");
                        if coord >= 0 && (coord as usize) < csv.len() {
                            csv[coord as usize] = n(cell, "v") + 1;
                            presence.set(coord as usize, csv[coord as usize] == 0);
                        }
                    }
                    if d["type"] == "IntGrid" {
                        grid_presence.insert(
                            li["iid"].as_str().expect("migrated layer IID").into(),
                            presence,
                        );
                    }
                    li["intGridCsv"] = json!(csv);
                }
                let td = li["overrideTilesetUid"]
                    .as_i64()
                    .or_else(|| d["tilesetDefUid"].as_i64())
                    .and_then(|id| tilesets.get(&id));
                for tile in li["gridTiles"].as_array_mut().unwrap() {
                    if tile["d"].is_null() {
                        tile["d"] = json!([tile["coordId"], tile["tileId"]]);
                    }
                    if tile["t"].is_null() || before(&version, 0, 6) {
                        tile["t"] = tile["d"][1].clone();
                    }
                    let coord = tile["d"][0].as_i64().unwrap_or(0);
                    if tile["px"].is_null() {
                        tile["px"] = json!([coord % cw.max(1) * grid, coord / cw.max(1) * grid]);
                    }
                    if tile["src"].is_null()
                        && let Some(td) = td
                    {
                        let r = rect(td, n(tile, "t"));
                        tile["src"] = json!([r["x"], r["y"]]);
                    }
                    defaults(tile, &json!({"f":0,"a":1}));
                }
                let cached_tiles = &mut li["autoLayerTiles"];
                if cached_tiles.is_null() {
                    *cached_tiles = json!([]);
                }
                let cached_tiles = cached_tiles
                    .as_array_mut()
                    .ok_or_else(|| Error::Invalid("auto-layer tiles must be an array".into()))?;
                for tile in cached_tiles {
                    if before(&version, 0, 6) {
                        tile["t"] = tile["d"][2].clone();
                    }
                    if before(&version, 0, 5) {
                        tile["px"][0] = json!(tile["px"][0].as_i64().unwrap_or(0) - ox);
                        tile["px"][1] = json!(tile["px"][1].as_i64().unwrap_or(0) - oy);
                    }
                    defaults(tile, &json!({"a":1}));
                }
                for e in li["entityInstances"].as_array_mut().unwrap() {
                    alias(e, "defId", "defUid");
                    uuid(e);
                    if e.get("x").is_some() {
                        e["px"] = json!([n(e, "x"), n(e, "y")]);
                    }
                    let definition = entity_defs.get(&n(e, "defUid"));
                    if let Some(d) = definition {
                        defaults(
                            e,
                            &json!({"width":d["width"],"height":d["height"],"fieldInstances":[],"__tile":d["tileRect"],"__smartColor":d["color"]}),
                        );
                    }
                    reconcile_field_instances(
                        &mut e["fieldInstances"],
                        definition.map(|d| array(&d["fieldDefs"])),
                        &field_defs,
                    )?;
                }
            }
        }
        for world in p.json["worlds"].as_array_mut().into_iter().flatten() {
            uuid(world);
            default_world_dimensions(world, &template.json);
        }
        p.json["jsonVersion"] = json!("1.5.4");
        p.grid_presence = grid_presence;
        p.refresh_derived_data()?;
        let mut changed = p.json != self.json || p.external_levels != self.external_levels;
        if changed && self.json["jsonVersion"] == "1.5.4" {
            let compatibility = super::OutputCompatibility::new(&p.json);
            if !compatibility.multiline && !compatibility.text_arrays.is_empty() {
                // Current files may use String for F_Text. Hydrate the editor
                // type without requiring a save when this is the only difference.
                compatibility.apply_project_text_types(&mut p, false);
                changed = p.json != self.json || p.external_levels != self.external_levels;
                compatibility.apply_project_text_types(&mut p, true);
            }
        }
        changed |= std::mem::take(&mut p.repaired_external_paths);
        p.external_levels
            .reuse_equal_documents(&self.external_levels);
        *self = p;
        Ok(changed)
    }
}
