use super::*;
use ldtk_core::clipboard::{ClipboardData, ClipboardKind};
use std::sync::Arc;

const CLIPBOARD_VERSION: &str = "1.5.4";

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

impl ClipTarget {
    fn accepts_rule_clipboard(&self, kind: ClipboardKind) -> bool {
        matches!(self, Self::Rule { rule, .. }
            if kind == ClipboardKind::Rule || rule.is_none() && kind == ClipboardKind::RuleGroup)
    }
}

#[derive(Clone, Default)]
struct RuleClipboardPreview {
    menu: Option<egui::Id>,
    shown_frame: u64,
    read_frame: Option<u64>,
    data: Option<Arc<ClipboardData>>,
}

fn rule_preview_id() -> egui::Id {
    egui::Id::new("source-rule-clipboard-preview")
}

fn rule_clipboard_preview(ui: &egui::Ui) -> Option<Arc<ClipboardData>> {
    let ctx = ui.ctx();
    let frame = ctx.cumulative_frame_nr();
    let (query, waiting, data) = ctx.data_mut(|mem| {
        let preview = mem.get_temp_mut_or_default::<RuleClipboardPreview>(rule_preview_id());
        let opened = preview.menu != Some(ui.id()) || preview.shown_frame + 1 < frame;
        preview.menu = Some(ui.id());
        preview.shown_frame = frame;
        let query = opened && preview.read_frame.is_none();
        if query {
            preview.data = None;
            // eframe queues the viewport request, then adds its paste event after
            // rendering the next frame, before taking the following raw input.
            preview.read_frame = Some(frame + 2);
        }
        (query, preview.read_frame.is_some(), preview.data.clone())
    });
    if query {
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    if waiting {
        ctx.request_repaint();
    }
    data
}

fn focus_id() -> egui::Id {
    egui::Id::new("source-definition-clipboard-focus")
}
fn pending_id() -> egui::Id {
    egui::Id::new("source-definition-paste-target")
}
impl EditorApp {
    pub(in crate::app) fn capture_rule_clipboard_preview(
        &self,
        ctx: &egui::Context,
        input: &mut egui::RawInput,
    ) {
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|mem| {
            let preview = mem.get_temp_mut_or_default::<RuleClipboardPreview>(rule_preview_id());
            if preview.read_frame != Some(frame) {
                return;
            }
            preview.read_frame = None;
            if let Some(egui::Event::Paste(raw)) = input.events.first() {
                preview.data = ClipboardData::decode(raw).ok().map(Arc::new);
                input.events.remove(0);
            }
        });
    }

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
    fn clipboard_is_embedded_tileset(&self, target: &ClipTarget) -> bool {
        matches!(
            target,
            ClipTarget::Definition { kind, uid: Some(uid) }
                if kind == "tilesets"
                    && self.project.as_ref().and_then(|p| p.tileset(*uid))
                        .is_some_and(|tileset| !tileset["embedAtlas"].is_null())
        )
    }

    pub(super) fn clipboard_copy(&mut self, ctx: &egui::Context, target: &ClipTarget, cut: bool) {
        if self.clipboard_is_embedded_tileset(target) {
            return;
        }
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
        match result.and_then(|d| d.encode(CLIPBOARD_VERSION)) {
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
        self.clipboard_apply(target, &data);
    }

    fn clipboard_apply(&mut self, target: &ClipTarget, data: &ClipboardData) {
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
                self.edit("Definition pasted", |p| p.paste_definition(data, *uid))
            }
            ClipTarget::Field { owner, uid } => self.edit("Field pasted", |p| {
                p.paste_field_definition(*owner, data, *uid)
            }),
            ClipTarget::Rule { layer, group, rule } => {
                if !target.accepts_rule_clipboard(data.kind) {
                    return;
                }
                let after = if data.kind == ClipboardKind::RuleGroup {
                    Some(*group)
                } else {
                    *rule
                };
                self.edit("Rule pasted", |p| {
                    p.paste_auto_rule(*layer, *group, data, after)
                })
            }
        };
        if let Some(uid) = result {
            match target {
                ClipTarget::Definition { .. } => {
                    self.definition_uid = Some(uid);
                }
                ClipTarget::Field { .. } => self.field_uid = Some(uid),
                ClipTarget::Rule { group, .. } => {
                    if data.kind == ClipboardKind::RuleGroup {
                        self.rule_group_uid = Some(uid);
                        self.rule_uid = None;
                    } else {
                        self.rule_group_uid = Some(*group);
                        self.rule_uid = Some(uid);
                    }
                }
            }
        }
    }
    pub(super) fn clipboard_duplicate(&mut self, target: &ClipTarget) {
        if self.clipboard_is_embedded_tileset(target) {
            return;
        }
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
        if let Ok(data) = data {
            self.clipboard_apply(target, &data);
        }
    }
    pub(super) fn clipboard_menu(&mut self, ui: &mut egui::Ui, target: ClipTarget) {
        if let ClipTarget::Rule { rule, .. } = &target {
            self.rule_clipboard_menu(ui, &target, rule.is_none());
            return;
        }
        let can_copy = !self.clipboard_is_embedded_tileset(&target);
        if can_copy && source_controls::button(ui, "Copy  ⌘C").clicked() {
            self.clipboard_copy(ui.ctx(), &target, false);
            ui.close();
        }
        if can_copy && source_controls::button(ui, "Cut  ⌘X").clicked() {
            self.clipboard_copy(ui.ctx(), &target, true);
            ui.close();
        }
        if source_controls::button(ui, "Paste  ⌘V").clicked() {
            self.clipboard_request_paste(ui.ctx(), target.clone());
            ui.close();
        }
        if can_copy && source_controls::button(ui, "Duplicate  ⌘D").clicked() {
            self.clipboard_duplicate(&target);
            ui.close();
        }
        if source_controls::button(ui, "Delete").clicked() {
            self.clipboard_delete(&target);
            ui.close();
        }
        ui.separator();
    }

    pub(super) fn rule_list_paste_menu(&mut self, ui: &mut egui::Ui, layer: i64) {
        let paste = rule_clipboard_preview(ui).filter(|data| data.kind == ClipboardKind::RuleGroup);
        let label = egui::RichText::new("PASTE GROUP").size(13.);
        let label = if paste.is_some() {
            label.color(theme::ACCENT)
        } else {
            label.italics().color(theme::TEXT)
        };
        ui.spacing_mut().button_padding.x = 20.;
        let response = ui.add_enabled(
            paste.is_some(),
            egui::Button::new(label)
                .frame_when_inactive(false)
                .min_size(egui::vec2(134., 32.)),
        );
        if let Some(data) = paste {
            if response.clicked() {
                if let Some(uid) = self.edit("Rule group pasted", |p| {
                    p.paste_auto_rule_group(layer, &data, None)
                }) {
                    self.rule_group_uid = Some(uid);
                    self.rule_uid = None;
                }
                ui.close();
            }
        } else {
            source_controls::disabled_hatch(ui, response.rect);
        }
    }

    fn rule_clipboard_menu(&mut self, ui: &mut egui::Ui, target: &ClipTarget, group: bool) {
        enum Action {
            Copy,
            Cut,
            Paste,
            Duplicate,
            Delete,
        }

        let paste =
            rule_clipboard_preview(ui).filter(|data| target.accepts_rule_clipboard(data.kind));
        let (width, height) = if group { (78., 38.) } else { (46., 33.) };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            let icon = |source| {
                egui::Button::image(
                    egui::Image::new(source).fit_to_exact_size(egui::Vec2::splat(20.)),
                )
            };
            let buttons = [
                (
                    Action::Copy,
                    icon(egui::include_image!("../../assets/icons/copy.svg")),
                    "Copy",
                ),
                (
                    Action::Cut,
                    icon(egui::include_image!("../../assets/icons/cut.svg")),
                    "Cut",
                ),
                (
                    Action::Paste,
                    icon(egui::include_image!("../../assets/icons/paste.svg")),
                    "Paste",
                ),
                (
                    Action::Duplicate,
                    egui::Button::new(egui::RichText::new("x2").strong()),
                    "Duplicate",
                ),
                (
                    Action::Delete,
                    icon(egui::include_image!("../../assets/icons/delete.svg")),
                    "Delete",
                ),
            ];
            for (action, button, label) in buttons {
                let enabled = !matches!(action, Action::Paste) || paste.is_some();
                let response = ui.add_enabled(
                    enabled,
                    button
                        .frame_when_inactive(false)
                        .min_size(egui::vec2(width, height)),
                );
                if !enabled {
                    source_controls::disabled_hatch(ui, response.rect);
                }
                if response.on_hover_text(label).clicked() {
                    match action {
                        Action::Copy => self.clipboard_copy(ui.ctx(), target, false),
                        Action::Cut => self.clipboard_copy(ui.ctx(), target, true),
                        Action::Paste => {
                            if let Some(data) = &paste {
                                self.clipboard_apply(target, data);
                            }
                        }
                        Action::Duplicate => self.clipboard_duplicate(target),
                        Action::Delete => self.clipboard_delete(target),
                    }
                    ui.close();
                }
            }
        });
    }
    pub(in crate::app) fn definition_clipboard_shortcuts(&mut self, ctx: &egui::Context) -> bool {
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
    fn preview_154_rule_copy_writes_the_upstream_signature() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
        let group = p.add_auto_rule_group(layer, "Source").unwrap();
        let rule = p.add_auto_rule(layer, group, 1).unwrap();
        app.set_project(p);
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.clipboard_copy(
                ctx,
                &ClipTarget::Rule {
                    layer,
                    group,
                    rule: Some(rule),
                },
                false,
            );
        });
        let [egui::OutputCommand::CopyText(text)] = output.platform_output.commands.as_slice()
        else {
            panic!("expected a system clipboard write");
        };
        assert!(
            text.starts_with("LDtk 1.5.4 clipboard\n----\nCRule\n----\n"),
            "{text}"
        );
    }

    #[test]
    fn preview_154_rule_row_rejects_group_paste() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
        let group = p.add_auto_rule_group(layer, "Source").unwrap();
        let rule = p.add_auto_rule(layer, group, 1).unwrap();
        let raw = p
            .copy_auto_rule(layer, group, None)
            .unwrap()
            .encode("1.5.4")
            .unwrap();
        let original = p.json.clone();
        app.set_project(p);
        app.panel = Some(Panel::Rules);
        app.definition_uid = Some(layer);
        app.rule_group_uid = Some(group);
        app.rule_uid = Some(rule);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste(raw)],
                ..Default::default()
            },
            |ctx| assert!(app.definition_clipboard_shortcuts(ctx)),
        );
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }

    #[test]
    fn rule_menu_preview_consumes_only_the_queued_clipboard_read() {
        let ctx = egui::Context::default();
        let app = EditorApp::empty();
        let show_menu = |ctx: &egui::Context, expected| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert_eq!(rule_clipboard_preview(ui).map(|data| data.kind), expected);
            });
        };
        let _ = ctx.run(egui::RawInput::default(), |ctx| show_menu(ctx, None));
        let _ = ctx.run(egui::RawInput::default(), |ctx| show_menu(ctx, None));

        let paste = egui::Event::Paste("keyboard paste".into());
        let mut input = egui::RawInput {
            events: vec![
                egui::Event::Paste("LDtk 1.5.4 clipboard\n----\nCRuleGroup\n----\n{}".into()),
                paste.clone(),
            ],
            ..Default::default()
        };
        app.capture_rule_clipboard_preview(&ctx, &mut input);
        assert_eq!(input.events, vec![paste]);
        let _ = ctx.run(input, |ctx| show_menu(ctx, Some(ClipboardKind::RuleGroup)));
    }

    #[test]
    fn preview_154_os_paste_rule_into_empty_group_is_undoable() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
        let source = p.add_auto_rule_group(layer, "Source").unwrap();
        let rule = p.add_auto_rule(layer, source, 1).unwrap();
        let dest = p.add_auto_rule_group(layer, "Empty").unwrap();
        let raw = p
            .copy_auto_rule(layer, source, Some(rule))
            .unwrap()
            .encode("1.5.4")
            .unwrap();
        app.set_project(p);
        app.panel = Some(Panel::Rules);
        app.definition_uid = Some(layer);
        app.rule_group_uid = Some(dest);
        app.rule_uid = None;
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste(raw)],
                ..Default::default()
            },
            |ctx| assert!(app.definition_clipboard_shortcuts(ctx)),
        );
        let rules = &app.project.as_ref().unwrap().layer_def(layer).unwrap()["autoRuleGroups"];
        assert_eq!(
            rules
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["uid"] == dest)
                .unwrap()["rules"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        let rules = &app.project.as_ref().unwrap().layer_def(layer).unwrap()["autoRuleGroups"];
        assert_eq!(
            rules
                .as_array()
                .unwrap()
                .iter()
                .find(|g| g["uid"] == dest)
                .unwrap()["rules"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

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
