use super::*;
use serde_json::json;

pub(super) fn drag_position(
    origin: [i32; 2],
    start: [f32; 2],
    end: [f32; 2],
    grid: f32,
    scale: f32,
    snap: bool,
) -> [i32; 2] {
    std::array::from_fn(|axis| {
        let delta = if snap {
            ((end[axis] / grid).floor() - (start[axis] / grid).floor()) * grid * scale
        } else {
            (end[axis] - start[axis]) * scale
        };
        origin[axis] + delta.trunc() as i32
    })
}

impl EditorApp {
    pub(super) fn try_chain_to(
        &mut self,
        key: LevelKey,
        layer: usize,
        index: usize,
        created: bool,
        brush_uid: Option<i64>,
    ) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(target) = project
            .layer(key, layer)
            .and_then(|l| l["entityInstances"].get(index))
            .cloned()
        else {
            return;
        };
        let Some(target_iid) = target["iid"].as_str().map(str::to_owned) else {
            return;
        };
        let Some(previous) = self.chain_entity.clone() else {
            if created {
                self.chain_entity = Some(target_iid);
            }
            return;
        };
        let source = project.level_keys().into_iter().find_map(|key| {
            project.level(key)?["layerInstances"]
                .as_array()?
                .iter()
                .find_map(|l| {
                    l["entityInstances"]
                        .as_array()?
                        .iter()
                        .find(|e| e["iid"] == previous)
                        .cloned()
                        .map(|e| (key, e))
                })
        });
        let Some((source_key, source)) = source else {
            self.chain_entity = created.then_some(target_iid);
            return;
        };
        let accepts = |definition: &Value, target_uid: i64| {
            if definition["allowOutOfLevelRef"] == false && source_key != key {
                return false;
            }
            let target = project.json["defs"]["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["uid"].as_i64() == Some(target_uid));
            match text(definition, "allowedRefs", "Any") {
                "OnlySame" => source["defUid"].as_i64() == Some(target_uid),
                "OnlySpecificEntity" => {
                    definition["allowedRefsEntityUid"].as_i64() == Some(target_uid)
                }
                "OnlyTags" => definition["allowedRefTags"].as_array().is_some_and(|tags| {
                    tags.iter().any(|tag| {
                        target
                            .and_then(|d| d["tags"].as_array())
                            .is_some_and(|ts| ts.contains(tag))
                    })
                }),
                _ => true,
            }
        };
        let target_uid = target["defUid"].as_i64().unwrap_or(-1);
        let candidate = project.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["uid"] == source["defUid"])
            .and_then(|d| d["fieldDefs"].as_array())
            .into_iter()
            .flatten()
            .find(|d| {
                d["autoChainRef"] == true
                    && matches!(d["__type"].as_str(), Some("EntityRef" | "Array<EntityRef>"))
                    && accepts(d, brush_uid.unwrap_or(target_uid))
            })
            .cloned();
        let Some(definition) = candidate else {
            if created {
                self.chain_entity = Some(target_iid);
            }
            return;
        };
        if !accepts(&definition, target_uid) {
            if created {
                self.chain_entity = None;
            }
            return;
        }
        let uid = definition["uid"].as_i64().unwrap_or(-1);
        let target_level = project.level(key).unwrap();
        let target_layer = project.layer(key, layer).unwrap();
        let reference = json!({"entityIid":target_iid,"layerIid":target_layer["iid"],"levelIid":target_level["iid"],"worldIid":key.world.map(|w|project.json["worlds"][w]["iid"].clone()).unwrap_or_else(||project.json["dummyWorldIid"].clone())});
        let is_array = definition["isArray"] == true;
        let count = source["fieldInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["defUid"] == uid)
            .and_then(|f| f["__value"].as_array())
            .map_or(0, Vec::len);
        if let Some(source_layer) = project
            .level(source_key)
            .and_then(|l| l["layerInstances"].as_array())
            .and_then(|ls| {
                ls.iter().position(|l| {
                    l["entityInstances"]
                        .as_array()
                        .is_some_and(|es| es.iter().any(|e| e["iid"] == previous))
                })
            })
        {
            self.history
                .mark_entity_change(project, source_key, source_layer, &source);
        }
        let project = self.project.as_mut().unwrap();
        let result = if is_array {
            project.edit_document(|p| {
                p.edit_field_array(
                    source_key,
                    Some(&previous),
                    uid,
                    ldtk_core::FieldArrayEdit::Insert { index: count },
                )?;
                p.edit_field_array(
                    source_key,
                    Some(&previous),
                    uid,
                    ldtk_core::FieldArrayEdit::Set {
                        index: count,
                        value: reference,
                    },
                )
            })
        } else {
            project.set_field_editor_value(source_key, Some(&previous), uid, reference)
        };
        match result {
            Ok(()) => {
                if definition["isArray"] == true || definition["symmetricalRef"] != true {
                    self.chain_entity = Some(target_iid);
                } else if created {
                    self.chain_entity = None;
                }
            }
            Err(_) => {
                if created {
                    self.chain_entity = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_creation_starts_chaining_and_navigation_preserves_valid_cross_level_chain() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Waypoint", 16, 16).unwrap();
        p.add_field_definition(Some(uid), "Next", "EntityRef", false)
            .unwrap();
        let a = p.level_keys()[0];
        let b = p.add_level(None, "Other", 256, 256).unwrap();
        p.add_entity(a, 0, uid, 8, 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(uid);
        app.try_chain_to(a, 0, 0, false, Some(uid));
        assert!(app.chain_entity.is_none());
        app.try_chain_to(a, 0, 0, true, Some(uid));
        let old = app.chain_entity.clone().unwrap();
        app.navigate_level(b, true);
        assert_eq!(app.chain_entity.as_deref(), Some(old.as_str()));
        app.edit_entity([24.0, 24.0], false);
        let p = app.project.as_ref().unwrap();
        let target = &p.layer(b, 0).unwrap()["entityInstances"][0];
        assert_eq!(
            p.layer(a, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"]["entityIid"],
            target["iid"]
        );
    }
}
