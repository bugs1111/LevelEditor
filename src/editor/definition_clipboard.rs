use super::*;
use ldtk_core::clipboard::{ClipboardData, ClipboardKind};
#[derive(Clone, Debug)]
pub(super) enum ClipTarget {
    Definition {
        kind: String,
        uid: Option<i64>,
    },
    Field {
        owner: Option<i64>,
        uid: Option<i64>,
    },
    Rule {
        layer: i64,
        group: i64,
        rule: Option<i64>,
    },
}
fn focus_id() -> egui::Id {
    egui::Id::new("source-definition-clipboard-focus")
}
fn pending_id() -> egui::Id {
    egui::Id::new("source-definition-paste-target")
}
impl EditorApp {
    pub(super) fn clipboard_focus(&self, ctx: &egui::Context, target: ClipTarget) {
        ctx.data_mut(|d| d.insert_temp(focus_id(), target));
    }
    fn clipboard_target(&self, ctx: &egui::Context) -> Option<ClipTarget> {
        let panel = self.panel?;
        if let Some(target) = ctx.data_mut(|d| d.get_temp::<ClipTarget>(focus_id())) {
            let valid = match &target {
                ClipTarget::Field { owner, .. } => {
                    panel == Panel::Fields
                        || panel == Panel::Entities && *owner == self.definition_uid
                }
                ClipTarget::Rule { .. } => panel == Panel::Rules,
                ClipTarget::Definition { kind, .. } => matches!(
                    (panel, kind.as_str()),
                    (Panel::Entities, "entities")
                        | (Panel::Layers, "layers")
                        | (Panel::Tilesets, "tilesets")
                        | (Panel::Enums, "enums")
                ),
            };
            if valid {
                return Some(target);
            }
        }
        Some(match panel {
            Panel::Entities => ClipTarget::Definition {
                kind: "entities".into(),
                uid: self.definition_uid,
            },
            Panel::Layers => ClipTarget::Definition {
                kind: "layers".into(),
                uid: self.definition_uid,
            },
            Panel::Enums => ClipTarget::Definition {
                kind: "enums".into(),
                uid: self.definition_uid,
            },
            Panel::Tilesets => ClipTarget::Definition {
                kind: "tilesets".into(),
                uid: self.definition_uid,
            },
            Panel::Fields => ClipTarget::Field {
                owner: self.field_owner,
                uid: self.field_uid,
            },
            Panel::Rules => ClipTarget::Rule {
                layer: self.definition_uid?,
                group: self.rule_group_uid?,
                rule: self.rule_uid,
            },
            _ => return None,
        })
    }
    pub(super) fn clipboard_copy(&mut self, ctx: &egui::Context, target: &ClipTarget, cut: bool) {
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let result = match target {
            ClipTarget::Definition {
                kind,
                uid: Some(uid),
            } => p.copy_definition(kind, *uid),
            ClipTarget::Field { uid: Some(uid), .. } => p.copy_field_definition(*uid),
            ClipTarget::Rule { layer, group, rule } => p.copy_auto_rule(*layer, *group, *rule),
            _ => return,
        };
        match result.and_then(|d| d.encode("1.5.3")) {
            Ok(raw) => {
                ctx.copy_text(raw);
                self.status = "Copied to system clipboard".into();
                self.status_error = false;
                if cut {
                    self.clipboard_delete(target);
                }
            }
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
            }
        }
    }
    pub(super) fn clipboard_delete(&mut self, target: &ClipTarget) {
        match target {
            ClipTarget::Definition {
                kind,
                uid: Some(uid),
            } => {
                self.edit_recoverable("Definition cut", |p| p.remove_definition(kind, *uid));
                self.definition_uid = None;
            }
            ClipTarget::Field { uid: Some(uid), .. } => {
                self.edit_recoverable("Field cut", |p| p.remove_field_definition(*uid));
                self.field_uid = None;
            }
            ClipTarget::Rule {
                layer,
                group,
                rule: Some(rule),
            } => {
                self.edit("Rule cut", |p| p.remove_auto_rule(*layer, *group, *rule));
                self.rule_uid = None;
            }
            ClipTarget::Rule {
                layer,
                group,
                rule: None,
            } => {
                self.edit_recoverable("Rule group cut", |p| {
                    p.remove_auto_rule_group(*layer, *group)
                });
                self.rule_group_uid = None;
            }
            _ => {}
        }
    }
    pub(super) fn clipboard_request_paste(&self, ctx: &egui::Context, target: ClipTarget) {
        ctx.data_mut(|d| d.insert_temp(pending_id(), target));
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    fn clipboard_paste(&mut self, raw: &str, target: &ClipTarget) {
        let data = match ClipboardData::decode(raw) {
            Ok(d) => d,
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
                return;
            }
        };
        let result = match target {
            ClipTarget::Definition { kind, uid } => {
                let expected = match kind.as_str() {
                    "entities" => ClipboardKind::EntityDef,
                    "layers" => ClipboardKind::LayerDef,
                    "enums" => ClipboardKind::EnumDef,
                    "tilesets" => ClipboardKind::TilesetDef,
                    _ => return,
                };
                if data.kind != expected {
                    self.status = "Clipboard type does not match this definition list".into();
                    self.status_error = true;
                    return;
                }
                self.edit("Definition pasted", |p| p.paste_definition(&data, *uid))
            }
            ClipTarget::Field { owner, uid } => self.edit("Field pasted", |p| {
                p.paste_field_definition(*owner, &data, *uid)
            }),
            ClipTarget::Rule { layer, group, rule } => self.edit("Rule pasted", |p| {
                p.paste_auto_rule(
                    *layer,
                    *group,
                    &data,
                    if data.kind == ClipboardKind::RuleGroup {
                        Some(*group)
                    } else {
                        *rule
                    },
                )
            }),
        };
        if let Some(uid) = result {
            match target {
                ClipTarget::Definition { .. } => {
                    self.definition_uid = Some(uid);
                }
                ClipTarget::Field { .. } => self.field_uid = Some(uid),
                ClipTarget::Rule { .. } => {
                    if data.kind == ClipboardKind::RuleGroup {
                        self.rule_group_uid = Some(uid);
                        self.rule_uid = None;
                    } else {
                        self.rule_uid = Some(uid);
                    }
                }
            }
        }
    }
    pub(super) fn clipboard_duplicate(&mut self, target: &ClipTarget) {
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let data = match target {
            ClipTarget::Definition {
                kind,
                uid: Some(uid),
            } => p.copy_definition(kind, *uid),
            ClipTarget::Field { uid: Some(uid), .. } => p.copy_field_definition(*uid),
            ClipTarget::Rule { layer, group, rule } => p.copy_auto_rule(*layer, *group, *rule),
            _ => return,
        };
        if let Ok(raw) = data.and_then(|d| d.encode("1.5.3")) {
            self.clipboard_paste(&raw, target);
        }
    }
    pub(super) fn clipboard_menu(&mut self, ui: &mut egui::Ui, target: ClipTarget) {
        if source_controls::button(ui, "Copy  ⌘C").clicked() {
            self.clipboard_copy(ui.ctx(), &target, false);
            ui.close();
        }
        if source_controls::button(ui, "Cut  ⌘X").clicked() {
            self.clipboard_copy(ui.ctx(), &target, true);
            ui.close();
        }
        if source_controls::button(ui, "Paste  ⌘V").clicked() {
            self.clipboard_request_paste(ui.ctx(), target.clone());
            ui.close();
        }
        if source_controls::button(ui, "Duplicate  ⌘D").clicked() {
            self.clipboard_duplicate(&target);
            ui.close();
        }
        if source_controls::button(ui, "Delete").clicked() {
            self.clipboard_delete(&target);
            ui.close();
        }
        ui.separator();
    }
    pub(in crate::app) fn definition_clipboard_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if source_colors::handle_color_clipboard(ctx) {
            return true;
        }
        if ctx.wants_keyboard_input() {
            return false;
        }
        let Some(target) = self.clipboard_target(ctx) else {
            return false;
        };
        let events = ctx.input(|i| i.events.clone());
        let mut handled = false;
        let has_copy = events.iter().any(|e| matches!(e, egui::Event::Copy));
        let has_cut = events.iter().any(|e| matches!(e, egui::Event::Cut));
        for event in events {
            match event {
                egui::Event::Copy => {
                    self.clipboard_copy(ctx, &target, false);
                    handled = true;
                }
                egui::Event::Cut => {
                    self.clipboard_copy(ctx, &target, true);
                    handled = true;
                }
                egui::Event::Paste(raw) => {
                    let pending = ctx
                        .data_mut(|d| {
                            let target = d.get_temp::<ClipTarget>(pending_id());
                            d.remove::<ClipTarget>(pending_id());
                            target
                        })
                        .unwrap_or(target.clone());
                    self.clipboard_paste(&raw, &pending);
                    handled = true;
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } if modifiers.command => match key {
                    Key::D => {
                        self.clipboard_duplicate(&target);
                        handled = true;
                    }
                    Key::C if !has_copy => {
                        self.clipboard_copy(ctx, &target, false);
                        handled = true;
                    }
                    Key::X if !has_cut => {
                        self.clipboard_copy(ctx, &target, true);
                        handled = true;
                    }
                    Key::V => {
                        handled = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        handled
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn os_paste_event_in_entity_panel_is_one_undoable_insert() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let raw = p
            .copy_definition("entities", uid)
            .unwrap()
            .encode("1.5.3")
            .unwrap();
        app.set_project(p);
        app.panel = Some(Panel::Entities);
        app.definition_uid = Some(uid);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste(raw)],
                ..Default::default()
            },
            |ctx| assert!(app.definition_clipboard_shortcuts(ctx)),
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["entities"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["entities"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
