use super::*;

pub(super) struct LayerVisibilityPress {
    key: LevelKey,
    origin: usize,
    paint: Option<bool>,
}

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
        let label = if value {
            "Single layer mode: ON"
        } else {
            "Single layer mode: off"
        };
        let message = self
            .repaint
            .as_ref()
            .map_or(label, |ctx| locale::tr_ctx(ctx, label));
        self.notify_quick(message.into());
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
        if !solo {
            self.set_layer_visibility(key, index, !visible);
            return;
        }
        let count = self
            .project
            .as_ref()
            .and_then(|p| p.level(key))
            .and_then(|l| l["layerInstances"].as_array())
            .map_or(0, Vec::len);
        let others = (0..count).filter(|i| *i != index);
        let show_all = visible && others.clone().all(|i| !self.layer_visible(key, i));
        for i in others {
            self.set_layer_visibility(key, i, show_all);
        }
        self.set_layer_visibility(key, index, true);
    }

    fn set_layer_visibility(&mut self, key: LevelKey, index: usize, visible: bool) {
        if self.layer_visible(key, index) == visible {
            return;
        }
        if let Some(project) = &mut self.project
            && project.layer(key, index).is_some()
        {
            self.history.begin_layer_visibility(project, key, index);
            let layer = project.layer_mut(key, index).unwrap();
            layer["visible"] = visible.into();
            self.hidden
                .remove(&layer["layerDefUid"].as_i64().unwrap_or(-1));
            self.history.commit(project);
            self.dirty = true;
            if let Some(ctx) = &self.repaint {
                ctx.request_repaint();
            }
        }
    }

    pub(super) fn handle_layer_visibility(&mut self, ui: &egui::Ui, rows: &[(usize, Rect, Rect)]) {
        let Some(key) = self.selected.filter(|_| ui.is_enabled()) else {
            self.layer_visibility_press = None;
            return;
        };
        let (events, down, focused) = ui.input(|input| {
            let events = input
                .events
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        egui::Event::PointerMoved(_)
                            | egui::Event::PointerButton {
                                button: egui::PointerButton::Primary,
                                ..
                            }
                            | egui::Event::PointerButton {
                                button: egui::PointerButton::Secondary,
                                pressed: true,
                                ..
                            }
                            | egui::Event::PointerGone
                            | egui::Event::WindowFocused(false)
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            (events, input.pointer.primary_down(), input.focused)
        });
        if !focused {
            self.layer_visibility_press = None;
            return;
        }
        let hit_row = |pos, eye_only| {
            if ui.ctx().layer_id_at(pos) != Some(ui.layer_id()) {
                return None;
            }
            rows.iter()
                .find(|(_, row, eye)| {
                    let rect = if eye_only { eye } else { row };
                    rect.intersect(ui.clip_rect()).contains(pos)
                })
                .map(|(index, _, _)| *index)
        };
        // Several eyes can receive pointer moves before the next frame.
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    pressed: true,
                    button,
                    modifiers,
                    ..
                } => {
                    self.layer_visibility_press = hit_row(pos, true)
                        .map(|index| {
                            let paint = (!modifiers.shift).then(|| !self.layer_visible(key, index));
                            self.toggle_layer_visibility(key, index, modifiers.shift);
                            LayerVisibilityPress {
                                key,
                                origin: index,
                                paint,
                            }
                        })
                        .filter(|_| {
                            button == egui::PointerButton::Primary
                                && !(cfg!(target_os = "macos") && modifiers.ctrl)
                        });
                }
                egui::Event::PointerMoved(pos) => {
                    if let Some(press) = &self.layer_visibility_press
                        && press.key == key
                        && let Some(visible) = press.paint
                        && let Some(index) = hit_row(pos, true)
                    {
                        self.set_layer_visibility(key, index, visible);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    pressed: false,
                    ..
                } => {
                    if let Some(press) = self.layer_visibility_press.take()
                        && press.key == key
                        && hit_row(pos, false) == Some(press.origin)
                    {
                        self.switch_layer(press.origin);
                        ui.ctx().request_repaint();
                    }
                }
                egui::Event::PointerGone | egui::Event::WindowFocused(false) => {
                    self.layer_visibility_press = None;
                }
                _ => {}
            }
        }
        if !down {
            self.layer_visibility_press = None;
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
