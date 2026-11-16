use super::*;

impl EditorApp {
    pub(super) fn set_single_layer_mode(&mut self, value: bool) {
        self.finish_gesture();
        self.finish_stroke();
        self.single_layer = value;
        self.settings.value["singleLayerMode"] = value.into();
        let _ = self.settings.save();
        self.selection = None;
        self.selection_group = None;
        self.entity_index = None;
    }
    pub(super) fn set_details(&mut self, value: bool) {
        self.finish_gesture();
        self.finish_stroke();
        self.details = value;
        self.settings.value["showDetails"] = value.into();
        let _ = self.settings.save();
        self.selection = None;
        self.selection_group = None;
        self.entity_index = None;
    }
    pub(super) fn set_empty_selection(&mut self, value: bool) {
        self.finish_gesture();
        self.finish_stroke();
        self.settings.value["emptySpaceSelection"] = value.into();
        let _ = self.settings.save();
        self.selection = None;
        self.selection_group = None;
        self.entity_index = None;
    }
    pub(super) fn layer_visible(&self, key: LevelKey, index: usize) -> bool {
        self.project
            .as_ref()
            .and_then(|p| p.layer(key, index))
            .is_some_and(|l| {
                l["visible"] != false
                    && !self
                        .hidden
                        .contains(&l["layerDefUid"].as_i64().unwrap_or(-1))
            })
    }
    pub(super) fn toggle_layer_visibility(&mut self, key: LevelKey, index: usize, solo: bool) {
        self.finish_gesture();
        self.finish_stroke();
        self.selection = None;
        self.selection_group = None;
        let visible = self.layer_visible(key, index);
        let all_others_hidden = self
            .project
            .as_ref()
            .and_then(|p| p.level(key))
            .and_then(|l| l["layerInstances"].as_array())
            .is_some_and(|layers| {
                (0..layers.len())
                    .filter(|i| *i != index)
                    .all(|i| !self.layer_visible(key, i))
            });
        if let Some(layers) = self
            .project
            .as_mut()
            .and_then(|p| p.level_mut(key))
            .and_then(|l| l["layerInstances"].as_array_mut())
        {
            for (i, layer) in layers.iter_mut().enumerate() {
                if solo {
                    layer["visible"] = (visible && all_others_hidden || i == index).into();
                } else if i == index {
                    layer["visible"] = (!visible).into();
                }
                self.hidden
                    .remove(&layer["layerDefUid"].as_i64().unwrap_or(-1));
            }
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_hidden_layers_can_be_shown_without_hiding_other_levels() {
        let mut p = Project::new();
        p.add_layer("Ground", "IntGrid", 16).unwrap();
        let first = p.level_keys()[0];
        let second = p.add_level(None, "Other", 256, 256).unwrap();
        p.layer_mut(first, 0).unwrap()["visible"] = false.into();
        let mut app = EditorApp::empty();
        app.set_project(p);
        assert!(!app.layer_visible(first, 0));
        app.toggle_layer_visibility(first, 0, false);
        assert!(app.layer_visible(first, 0));
        app.toggle_layer_visibility(first, 0, false);
        assert!(!app.layer_visible(first, 0));
        assert!(app.layer_visible(second, 0));
        assert!(app.dirty);
    }
}
