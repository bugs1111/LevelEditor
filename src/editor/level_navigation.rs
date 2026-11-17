use super::*;
use serde_json::json;

impl EditorApp {
    fn clear_level_selection(&mut self) {
        self.entity_index = None;
        self.selection = None;
        self.selection_group = None;
        self.stamp = None;
        self.hidden.clear();
        if !self.picking_entity_ref() {
            self.field_pick = None;
        }
        if self.panel == Some(Panel::Entity) {
            self.panel = None;
        }
    }
    pub(super) fn navigate_level(&mut self, key: LevelKey, fit: bool) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::Navigate { key, fit }) {
            return;
        }
        if self.project.as_ref().and_then(|p| p.level(key)).is_none() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.cancel_world_context();
        let layer_uid = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
            .map(|l| l["layerDefUid"].clone());
        self.clear_level_selection();
        self.selected = Some(key);
        self.active_world = key.world;
        let level = self.project.as_ref().unwrap().level(key).unwrap();
        self.layer = layer_uid
            .and_then(|uid| {
                level["layerInstances"]
                    .as_array()?
                    .iter()
                    .position(|l| l["layerDefUid"] == uid)
            })
            .unwrap_or(0);
        self.world_depth = level["worldDepth"].as_i64().unwrap_or(0);
        self.fit = fit;
        self.normalize_palette();
        self.remember_selected_level();
    }
    pub(super) fn navigate_world(&mut self, world: Option<usize>) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::World(world)) {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.cancel_world_context();
        if let Some(key) = self
            .project
            .as_ref()
            .and_then(|p| p.level_keys().into_iter().find(|key| key.world == world))
        {
            self.navigate_level(key, true);
        } else {
            self.clear_level_selection();
            self.selected = None;
            self.active_world = world;
            self.world_depth = 0;
            self.layer = 0;
            self.fit = true;
        }
        self.world = true;
    }
    pub(super) fn show_panel(&mut self, panel: Option<Panel>) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::Panel(panel)) {
            return;
        }
        if self.panel == Some(Panel::Settings) && panel != Some(Panel::Settings) {
            self.finish_settings_dialog(&ctx);
        }
        self.finish_gesture();
        self.finish_stroke();
        self.cancel_world_context();
        if panel == Some(Panel::World) {
            if !self.world {
                self.world = true;
                self.fit = true;
            }
        } else if self.panel == Some(Panel::World) || panel.is_none() && self.world {
            self.world = false;
            self.fit = true;
        }
        self.panel = panel;
    }

    pub(super) fn toggle_toolbar_panel(&mut self, panel: Panel) {
        let open = self.panel != Some(panel);
        self.show_panel(open.then_some(panel));
    }
    pub(super) fn follow_entity(&mut self, iid: &str) -> bool {
        let target = self.project.as_ref().and_then(|p| {
            p.level_keys().into_iter().find_map(|key| {
                p.level(key)?["layerInstances"]
                    .as_array()?
                    .iter()
                    .enumerate()
                    .find_map(|(layer, l)| {
                        l["entityInstances"]
                            .as_array()?
                            .iter()
                            .position(|e| e["iid"] == iid)
                            .map(|index| (key, layer, index))
                    })
            })
        });
        let Some((key, layer, index)) = target else {
            return false;
        };
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::FollowEntity(iid.into())) {
            return true;
        }
        self.navigate_level(key, false);
        self.world = false;
        self.layer = layer;
        self.entity_index = Some(index);
        self.panel = Some(Panel::Entity);
        if let Some(project) = &self.project
            && let Some(entity) = project
                .layer(key, layer)
                .and_then(|l| l["entityInstances"].get(index))
        {
            self.entity_uid = entity["defUid"].as_i64();
            self.bleep_entity(key, layer, entity);
            // followEntityRef also calls SelectionTool.select, which flashes again.
            self.bleep_entity(key, layer, entity);
            let geometries = self.selection_geometries(key);
            let regions =
                json!([{"layerIndex":layer,"rect":[0,0,1,1],"geometry":geometries[layer]}]);
            if let Ok(mut group) =
                project.copy_selection_group_from_regions(key, &regions, [0, 0, 1, 1])
            {
                group["layers"][0]["clipboard"]["entityInstances"] = json!([entity]);
                group["layers"][0]["points"] = json!([]);
                group["originalRects"] = json!([]);
                group["emptySpace"] = json!(false);
                if let Ok(group) =
                    project.refresh_selection_group_geometry(key, &group, &geometries)
                {
                    self.selection_group = Some(group);
                }
            }
            self.viewport.center = canvas::layer_to_world(
                project,
                key,
                layer,
                &self.viewport,
                [
                    entity["px"][0].as_f64().unwrap_or(0.0) as f32,
                    entity["px"][1].as_f64().unwrap_or(0.0) as f32,
                ],
            );
        }
        if let Some(group) = self.selection_group.take() {
            self.set_selection_group(group);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_never_carries_an_entity_index_or_selection_into_another_level() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let a = p.level_keys()[0];
        let b = p.add_level(None, "Second", 256, 256).unwrap();
        p.add_entity(a, 0, uid, 16, 16).unwrap();
        p.add_entity(b, 0, uid, 32, 32).unwrap();
        p.move_level(b, 0, 0, 3).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_index = Some(0);
        app.selection = Some([0, 0, 1, 1]);
        app.navigate_level(b, true);
        app.delete_selection();
        assert!(
            app.entity_index.is_none() && app.selection.is_none() && app.selection_group.is_none()
        );
        assert_eq!(app.world_depth, 3);
        assert_eq!(
            app.project.as_ref().unwrap().layer(b, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
