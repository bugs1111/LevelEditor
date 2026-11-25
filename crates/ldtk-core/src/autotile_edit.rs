//! Transactional rule-definition editing; evaluation remains in the parent module.
use super::{Rule, arr};
use crate::{Error, Project, Result, workflow::alloc_uid};
use serde_json::{Value, json};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn layer_mut(p: &mut Project, uid: i64) -> Result<&mut Value> {
    let layer = p.json["defs"]["layers"]
        .as_array_mut()
        .and_then(|v| v.iter_mut().find(|l| l["uid"] == uid))
        .ok_or_else(|| invalid("auto-layer definition not found"))?;
    if layer["type"] != "IntGrid" && layer["type"] != "AutoLayer" {
        return Err(invalid("rules require an IntGrid or AutoLayer definition"));
    }
    Ok(layer)
}
fn groups_mut(p: &mut Project, uid: i64) -> Result<&mut Vec<Value>> {
    let ld = layer_mut(p, uid)?;
    if ld["autoRuleGroups"].is_null() {
        ld["autoRuleGroups"] = json!([])
    }
    ld["autoRuleGroups"]
        .as_array_mut()
        .ok_or_else(|| invalid("autoRuleGroups must be an array"))
}
fn group_mut(p: &mut Project, layer: i64, uid: i64) -> Result<&mut Value> {
    groups_mut(p, layer)?
        .iter_mut()
        .find(|g| g["uid"] == uid)
        .ok_or_else(|| invalid("auto-rule group not found"))
}
fn rules_mut(p: &mut Project, layer: i64, group: i64) -> Result<&mut Vec<Value>> {
    group_mut(p, layer, group)?["rules"]
        .as_array_mut()
        .ok_or_else(|| invalid("rules must be an array"))
}
fn patch(target: &mut Value, patch: &Value, protected: &[&str]) -> Result<()> {
    let fields = patch
        .as_object()
        .ok_or_else(|| invalid("rule patch must be an object"))?;
    if protected.iter().any(|key| fields.contains_key(*key)) {
        return Err(invalid(
            "UIDs and child collections require explicit operations",
        ));
    }
    for (key, value) in fields {
        target[key] = value.clone();
    }
    Ok(())
}
fn default_rule(uid: i64, size: i32) -> Value {
    json!({"uid":uid,"active":true,"size":size,"tileRectsIds":[],"alpha":1,"chance":1,"breakOnMatch":true,"pattern":vec![0; (size*size) as usize],"flipX":false,"flipY":false,"xModulo":1,"yModulo":1,"xOffset":0,"yOffset":0,"tileXOffset":0,"tileYOffset":0,"tileRandomXMin":0,"tileRandomXMax":0,"tileRandomYMin":0,"tileRandomYMax":0,"checker":"None","tileMode":"Single","pivotX":0,"pivotY":0,"outOfBoundsValue":null,"invalidated":false,"perlinActive":false,"perlinSeed":(uuid::Uuid::new_v4().as_u128()%9_999_999) as i64,"perlinScale":0.2,"perlinOctaves":2})
}

pub(crate) fn clipboard_rule(uid: i64, changes: &Value) -> Result<Value> {
    let size = changes["size"]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .filter(|n| (1..=9).contains(n) && n % 2 == 1)
        .ok_or_else(|| invalid("rule size must be odd and between 1 and 9"))?;
    let mut rule = default_rule(uid, size);
    patch(&mut rule, changes, &["uid", "tileIds"])?;
    validate_rule(&rule)?;
    Ok(rule)
}

fn validate_rule(rule: &Value) -> Result<()> {
    Rule::new(rule)?;
    for key in [
        "active",
        "breakOnMatch",
        "flipX",
        "flipY",
        "perlinActive",
        "invalidated",
    ] {
        if let Some(value) = rule.get(key)
            && !value.is_boolean()
        {
            return Err(invalid("rule boolean property has incorrect type"));
        }
    }
    for key in ["alpha", "chance", "pivotX", "pivotY"] {
        if let Some(value) = rule.get(key)
            && value.as_f64().is_none_or(|v| !(0.0..=1.0).contains(&v))
        {
            return Err(invalid("rule fraction must be between zero and one"));
        }
    }
    if let Some(value) = rule.get("perlinScale")
        && value.as_f64().is_none_or(|v| v < 0.)
    {
        return Err(invalid("Perlin scale must be nonnegative"));
    }
    if let Some(value) = rule.get("perlinOctaves")
        && value.as_i64().is_none_or(|v| !(1..=32).contains(&v))
    {
        return Err(invalid("Perlin octaves must be between 1 and 32"));
    }
    if let Some(mode) = rule.get("tileMode")
        && mode != "Single"
        && mode != "Stamp"
    {
        return Err(invalid("invalid auto-rule tile mode"));
    }
    if let Some(mode) = rule.get("checker")
        && mode != "None"
        && mode != "Horizontal"
        && mode != "Vertical"
    {
        return Err(invalid("invalid auto-rule checker mode"));
    }
    let rects = rule["tileRectsIds"]
        .as_array()
        .ok_or_else(|| invalid("tileRectsIds must be an array"))?;
    if rects.iter().any(|r| r.as_array().is_none_or(Vec::is_empty)) {
        return Err(invalid("tile rectangles must be nonempty arrays"));
    }
    Ok(())
}
fn validate_group(group: &Value) -> Result<()> {
    for key in ["active", "isOptional", "usesWizard"] {
        if let Some(v) = group.get(key)
            && !v.is_boolean()
        {
            return Err(invalid("group boolean property has incorrect type"));
        }
    }
    if group["name"]
        .as_str()
        .is_none_or(|name| name.trim().is_empty())
    {
        return Err(invalid("rule group name must not be empty"));
    }
    if group["requiredBiomeValues"]
        .as_array()
        .is_none_or(|v| v.iter().any(|v| !v.is_string()))
    {
        return Err(invalid("requiredBiomeValues must contain strings"));
    }
    if !matches!(group["biomeRequirementMode"].as_i64(), Some(0 | 1)) {
        return Err(invalid("biome requirement mode must be OR (0) or AND (1)"));
    }
    Ok(())
}
fn remove(values: &mut Vec<Value>, uid: i64) -> Result<()> {
    let index = values
        .iter()
        .position(|v| v["uid"] == uid)
        .ok_or_else(|| invalid("rule or group not found"))?;
    values.remove(index);
    Ok(())
}
fn reorder(values: &mut Vec<Value>, uid: i64, index: usize) -> Result<()> {
    if index >= values.len() {
        return Err(invalid("rule reorder index outside list"));
    }
    let old = values
        .iter()
        .position(|v| v["uid"] == uid)
        .ok_or_else(|| invalid("rule or group not found"))?;
    let value = values.remove(old);
    values.insert(index, value);
    Ok(())
}
impl Project {
    fn edit_auto_definition<T>(
        &mut self,
        layer_uid: i64,
        edit: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.edit_document(|p| {
            let result = edit(p)?;
            for key in p.level_keys() {
                p.rebuild_auto_filtered(key, None, Some(layer_uid))?;
            }
            Ok(result)
        })
    }
    pub fn add_auto_rule_group(&mut self, layer_uid: i64, name: &str) -> Result<i64> {
        self.edit_auto_definition(layer_uid,|p|{
            if name.trim().is_empty(){return Err(invalid("rule group name must not be empty"))}
            let uid=alloc_uid(p)?;
            groups_mut(p,layer_uid)?.push(json!({"uid":uid,"name":name,"color":null,"icon":null,"active":true,"isOptional":false,"rules":[],"usesWizard":false,"requiredBiomeValues":[],"biomeRequirementMode":0}));Ok(uid)
        })
    }
    pub fn add_auto_rule(&mut self, layer_uid: i64, group_uid: i64, size: i32) -> Result<i64> {
        self.edit_auto_definition(layer_uid, |p| {
            if !(1..=9).contains(&size) || size % 2 == 0 {
                return Err(invalid("rule size must be odd and between 1 and 9"));
            }
            let uid = alloc_uid(p)?;
            rules_mut(p, layer_uid, group_uid)?.push(default_rule(uid, size));
            Ok(uid)
        })
    }
    /// Replace a wizard group and compile its affected instances once, as one undoable edit.
    pub fn apply_auto_rule_wizard(
        &mut self,
        layer_uid: i64,
        group_uid: Option<i64>,
        name: &str,
        rules: &[Value],
    ) -> Result<i64> {
        self.edit_auto_definition(layer_uid,|p|{
            if name.trim().is_empty(){return Err(invalid("rule group name must not be empty"));}
            let uid=if let Some(uid)=group_uid{group_mut(p,layer_uid,uid)?;uid}else{let uid=alloc_uid(p)?;groups_mut(p,layer_uid)?.insert(0,json!({"uid":uid,"name":name,"color":null,"icon":null,"active":true,"isOptional":false,"rules":[],"usesWizard":true,"requiredBiomeValues":[],"biomeRequirementMode":0}));uid};
            let source_uid=layer_mut(p,layer_uid)?["autoSourceLayerDefUid"].as_i64().unwrap_or(layer_uid);
            let known= p.layer_def(source_uid).map(|d|arr(&d["intGridValues"]).iter().filter_map(|v|v["value"].as_i64()).collect::<Vec<_>>()).unwrap_or_default();
            // Reserve one contiguous range; avoid rescanning the entire project for every fragment.
            let first=if rules.is_empty(){0}else{let first=alloc_uid(p)?;let end=first.checked_add(i64::try_from(rules.len()).map_err(|_|invalid("too many rules"))?).filter(|end|*end<i64::from(i32::MAX)).ok_or_else(||invalid("UID overflow"))?;p.json["nextUid"]=json!(end);first};
            let mut generated=Vec::with_capacity(rules.len());
            for (index,changes) in rules.iter().enumerate(){
                let size=changes["size"].as_i64().and_then(|n|i32::try_from(n).ok()).filter(|n|(1..=9).contains(n)&&n%2==1).ok_or_else(||invalid("rule size must be odd and between 1 and 9"))?;
                let mut rule=default_rule(first+index as i64,size);patch(&mut rule,changes,&["uid","tileIds"])?;validate_rule(&rule)?;
                let mut size=size as usize;let mut pattern=arr(&rule["pattern"]).to_vec();
                while size>1&&(0..size).all(|i|pattern[i]==0&&pattern[(size-1)*size+i]==0&&pattern[i*size]==0&&pattern[i*size+size-1]==0){let next=(1..size-1).flat_map(|y|(1..size-1).map(move|x|x+y*size)).map(|i|pattern[i].clone()).collect();pattern=next;size-=2;}
                rule["size"]=json!(size);rule["pattern"]=json!(pattern);
                let parsed=Rule::new(&rule)?;let flips=[parsed.flip_x,parsed.flip_y];rule["flipX"]=json!(flips[0]);rule["flipY"]=json!(flips[1]);
                let x=rule["xModulo"].as_i64().unwrap_or(1);let y=rule["yModulo"].as_i64().unwrap_or(1);
                if x==1&&y==1{rule["checker"]=json!("None");}else if x==1&&rule["checker"]=="Horizontal"{rule["checker"]=json!("Vertical");}else if y==1&&rule["checker"]=="Vertical"{rule["checker"]=json!("Horizontal");}
                if let Some(v)=rule["outOfBoundsValue"].as_i64()&&v!=0&&!known.contains(&v){rule["outOfBoundsValue"]=Value::Null;}
                generated.push(rule);
            }
            let group=group_mut(p,layer_uid,uid)?;group["name"]=json!(name);group["usesWizard"]=json!(true);group["rules"]=json!(generated);Ok(uid)
        })
    }
    pub fn update_auto_rule(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        rule_uid: i64,
        changes: &Value,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            let rule = rules_mut(p, layer_uid, group_uid)?
                .iter_mut()
                .find(|r| r["uid"] == rule_uid)
                .ok_or_else(|| invalid("auto-rule not found"))?;
            if rule["tileIds"].is_array() {
                rule["tileRectsIds"] = json!(Rule::new(rule)?.rects);
                rule.as_object_mut().expect("rule object").remove("tileIds");
            }
            patch(rule, changes, &["uid", "tileIds"])?;
            validate_rule(rule)
        })
    }
    /// Source group-menu operation: change every owned rule and rebuild once per affected level.
    pub fn set_rule_group_out_of_bounds(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        value: Option<i32>,
    ) -> Result<usize> {
        self.edit_auto_definition(layer_uid, |p| {
            let source_uid = layer_mut(p, layer_uid)?["autoSourceLayerDefUid"]
                .as_i64()
                .unwrap_or(layer_uid);
            if let Some(value) = value
                && (value < 0
                    || value > 0
                        && !p.layer_def(source_uid).is_some_and(|source| {
                            arr(&source["intGridValues"])
                                .iter()
                                .any(|v| v["value"] == value)
                        }))
            {
                return Err(invalid(
                    "out-of-bounds policy must be default, empty or a source IntGrid value",
                ));
            }
            let rules = rules_mut(p, layer_uid, group_uid)?;
            for rule in rules.iter_mut() {
                rule["outOfBoundsValue"] = json!(value);
            }
            Ok(rules.len())
        })
    }
    pub fn update_auto_rule_group(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        changes: &Value,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            let group = group_mut(p, layer_uid, group_uid)?;
            let was_optional = group["isOptional"] == true;
            patch(group, changes, &["uid", "rules"])?;
            validate_group(group)?;
            if was_optional && group["isOptional"] != true {
                for key in p.level_keys() {
                    let Some(layers) = p
                        .level_mut(key)
                        .and_then(|level| level["layerInstances"].as_array_mut())
                    else {
                        continue;
                    };
                    for layer in layers {
                        if layer["layerDefUid"] == layer_uid
                            && let Some(optional) = layer["optionalRules"].as_array_mut()
                        {
                            optional.retain(|uid| *uid != group_uid);
                        }
                    }
                }
            }
            Ok(())
        })
    }
    pub fn remove_auto_rule(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        rule_uid: i64,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            remove(rules_mut(p, layer_uid, group_uid)?, rule_uid)
        })
    }
    pub fn remove_auto_rule_group(&mut self, layer_uid: i64, group_uid: i64) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            remove(groups_mut(p, layer_uid)?, group_uid)?;
            for key in p.level_keys() {
                if let Some(layers) = p
                    .level_mut(key)
                    .and_then(|l| l["layerInstances"].as_array_mut())
                {
                    for li in layers {
                        if li["layerDefUid"] == layer_uid
                            && let Some(optional) = li["optionalRules"].as_array_mut()
                        {
                            optional.retain(|v| *v != group_uid);
                        }
                    }
                }
            }
            Ok(())
        })
    }
    pub fn move_auto_rule_to_group(
        &mut self,
        layer_uid: i64,
        from_group: i64,
        to_group: i64,
        rule_uid: i64,
        index: usize,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            let source = rules_mut(p, layer_uid, from_group)?;
            let old = source
                .iter()
                .position(|r| r["uid"] == rule_uid)
                .ok_or_else(|| invalid("auto-rule not found"))?;
            let rule = source.remove(old);
            let destination = rules_mut(p, layer_uid, to_group)?;
            if index > destination.len() {
                return Err(invalid("rule destination index outside list"));
            }
            destination.insert(index, rule);
            Ok(())
        })
    }
    pub fn reorder_auto_rule(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        rule_uid: i64,
        index: usize,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            reorder(rules_mut(p, layer_uid, group_uid)?, rule_uid, index)
        })
    }
    pub fn reorder_auto_rule_group(
        &mut self,
        layer_uid: i64,
        group_uid: i64,
        index: usize,
    ) -> Result<()> {
        self.edit_auto_definition(layer_uid, |p| {
            reorder(groups_mut(p, layer_uid)?, group_uid, index)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_rule_validates_before_batch_install_and_preserves_extensions() {
        let rule = clipboard_rule(
            45,
            &json!({"size":1,"pattern":[1],"tileRectsIds":[[2]],"extension":"kept"}),
        )
        .unwrap();
        assert_eq!(rule["uid"], 45);
        assert_eq!(rule["extension"], "kept");
        assert_eq!(rule["chance"], 1);
        assert!(clipboard_rule(45, &json!({"size":1,"pattern":[1],"uid":12})).is_err());
        assert!(clipboard_rule(45, &json!({"size":2,"pattern":[1,1,1,1]})).is_err());
    }
}
