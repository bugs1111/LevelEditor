use super::*;
use serde_json::json;

pub(super) fn placement_in_level(project: &Project, key: LevelKey, [x, y]: [f32; 2]) -> bool {
    let [left, top, right, bottom] = canvas::bounds(project, key, false);
    (left..right).contains(&x) && (top..bottom).contains(&y)
}

pub(super) fn drag_delta(
    project: &Project,
    key: LevelKey,
    layer: usize,
    view: &Viewport,
    [start, end]: [[f32; 2]; 2],
    snap: bool,
) -> [f32; 2] {
    if !snap {
        return [end[0] - start[0], end[1] - start[1]];
    }
    let grid = project
        .layer(key, layer)
        .map_or(16., |layer| number(layer, "__gridSize", 16.).max(1.));
    let scale =
        canvas::pointer_transform(project, key, layer, view).map_or(1., |transform| transform.scale);
    let [start, end] =
        [start, end].map(|level| canvas::pointer_to_layer(project, key, layer, view, level));
    std::array::from_fn(|axis| {
        ((end[axis] / grid).floor() - (start[axis] / grid).floor()) * (grid * scale)
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
                .enumerate()
                .find_map(|(layer, l)| {
                    l["entityInstances"]
                        .as_array()?
                        .iter()
                        .find(|e| e["iid"] == previous)
                        .cloned()
                        .map(|e| (key, layer, e))
                })
        });
        let Some((source_key, source_layer, source)) = source else {
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
        // LDtk saves placement first and only records the link for same-level pairs.
        self.dirty |= self.history.commit(project);
        if source_key == key {
            self.history.begin(project, key);
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
        if source_key == key {
            if result.is_ok() {
                self.dirty |= self.history.commit(project);
            } else {
                self.history.cancel(project);
            }
        } else {
            self.dirty |= result.is_ok();
        }
        if self.stroke {
            // Capture any move or resize that follows the press independently of the link.
            self.history.begin(project, key);
        }
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
        app.place_entity([24.0, 24.0], [24.0, 24.0]);
        let p = app.project.as_ref().unwrap();
        let target = &p.layer(b, 0).unwrap()["entityInstances"][0];
        assert_eq!(
            p.layer(a, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"]["entityIid"],
            target["iid"]
        );
    }
}
