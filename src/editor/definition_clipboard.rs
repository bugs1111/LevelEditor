use super::*;
use crate::app::chrome::ActionTooltip as _;
use ldtk_core::clipboard::{ClipboardData, ClipboardKind};
use std::sync::Arc;

const CLIPBOARD_VERSION: &str = "1.5.4";

#[derive(Clone)]
struct DefinitionRemoval {
    project_iid: Value,
    kind: String,
    uid: i64,
    warning: &'static str,
}

fn definition_removal_id() -> egui::Id {
    egui::Id::new("definition-removal")
}

fn definition_removal_warning(project: &Project, kind: &str, uid: i64) -> Option<&'static str> {
    let defs = &project.json["defs"];
    match kind {
        "layers"
            if defs["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|other| other["autoSourceLayerDefUid"] == uid) =>
        {
            Some(
                "Warning! This IntGrid layer is used by another one as SOURCE. Deleting it will disconnect the corresponding auto-layer(s).\nTheir rules will be kept and can be used again after selecting a new source.",
            )
        }
        "entities"
            if project
                .level_keys()
                .into_iter()
                .filter_map(|key| project.level(key))
                .flat_map(|level| level["layerInstances"].as_array().into_iter().flatten())
                .filter(|layer| layer["__type"] == "Entities")
                .flat_map(|layer| layer["entityInstances"].as_array().into_iter().flatten())
                .any(|entity| entity["defUid"] == uid) =>
        {
            Some(
                "WARNING! This entity is used in one or more levels. The corresponding instances will also be deleted!",
            )
        }
        "enums" if project.is_enum_definition_used(uid) => Some(
            "WARNING! This ENUM is used in one or more entity fields. These fields will also be deleted!",
        ),
        _ => None,
    }
}

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
    fn accepts_clipboard(&self, kind: ClipboardKind) -> bool {
        match self {
            Self::Rule { rule, .. } => {
                kind == ClipboardKind::Rule || rule.is_none() && kind == ClipboardKind::RuleGroup
            }
            Self::Field { .. } => kind == ClipboardKind::FieldDef,
            Self::Definition { kind: list, .. } => matches!(
                (list.as_str(), kind),
                ("layers", ClipboardKind::LayerDef)
                    | ("entities", ClipboardKind::EntityDef)
                    | ("enums", ClipboardKind::EnumDef)
                    | ("tilesets", ClipboardKind::TilesetDef)
            ),
        }
    }
}

#[derive(Clone, Default)]
struct ClipboardPreview {
    menu: Option<egui::Id>,
    shown_frame: u64,
    read_frame: Option<u64>,
    data: Option<Arc<ClipboardData>>,
}

fn clipboard_preview_id() -> egui::Id {
    egui::Id::new("source-clipboard-preview")
}

fn clipboard_object_name(ctx: &egui::Context, data: &ClipboardData) -> String {
    let (kind, name) = match data.kind {
        ClipboardKind::Rule => ("Rule", data.value["uid"].to_string()),
        ClipboardKind::RuleGroup => ("Rule group", text(&data.value, "name", "").to_owned()),
        kind => {
            let label = match kind {
                ClipboardKind::LayerDef => "Layer definition",
                ClipboardKind::EntityDef => "Entity definition",
                ClipboardKind::EnumDef => "Enum definition",
                ClipboardKind::TilesetDef => "Tileset definition",
                _ => "Field definition",
            };
            (label, text(&data.value, "identifier", "").to_owned())
        }
    };
    format!("{} {}", locale::tr_ctx(ctx, kind), name.replace('"', ""))
}

fn clipboard_preview(ui: &egui::Ui) -> Option<Arc<ClipboardData>> {
    let ctx = ui.ctx();
    let frame = ctx.cumulative_frame_nr();
    let (query, waiting, data) = ctx.data_mut(|mem| {
        let preview = mem.get_temp_mut_or_default::<ClipboardPreview>(clipboard_preview_id());
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
impl EditorApp {
    pub(in crate::app) fn capture_clipboard_preview(
        &self,
        ctx: &egui::Context,
        input: &mut egui::RawInput,
    ) {
        let frame = ctx.cumulative_frame_nr();
        ctx.data_mut(|mem| {
            let preview = mem.get_temp_mut_or_default::<ClipboardPreview>(clipboard_preview_id());
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
        match result.and_then(|data| {
            let raw = data.encode(CLIPBOARD_VERSION)?;
            Ok((raw, clipboard_object_name(ctx, &data)))
        }) {
            Ok((raw, name)) => {
                ctx.copy_text(raw);
                self.status.clear();
                self.status_error = false;
                self.report_canvas_notice(notifications::CanvasNotice::ClipboardCopied(name));
                if cut {
                    self.clipboard_delete(ctx, target);
                }
            }
            Err(e) => {
                self.status = e.to_string();
                self.status_error = true;
            }
        }
    }
    pub(super) fn request_definition_removal(&mut self, ctx: &egui::Context, kind: &str, uid: i64) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if !project.is_backup()
            && let Some(warning) = definition_removal_warning(project, kind, uid)
        {
            let pending = DefinitionRemoval {
                project_iid: project.json["iid"].clone(),
                kind: kind.to_owned(),
                uid,
                warning,
            };
            ctx.data_mut(|data| data.insert_temp(definition_removal_id(), pending));
        } else {
            self.remove_definition_from_editor(ctx, kind, uid);
        }
    }

    fn remove_definition_from_editor(&mut self, ctx: &egui::Context, kind: &str, uid: i64) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(definition) = project.json["defs"][kind]
            .as_array()
            .into_iter()
            .flatten()
            .find(|def| def["uid"] == uid)
        else {
            return;
        };
        let label = match kind {
            "layers" => "Layer {name} deleted",
            "entities" => "Entity {name} deleted",
            "enums" => "Enum {name} deleted",
            _ => "Tileset {name} deleted",
        };
        let title = locale::format_ctx(ctx, label, &[("name", text(definition, "identifier", ""))]);
        if self
            .edit_recoverable(&title, |p| p.remove_definition(kind, uid))
            .is_some()
        {
            let first = self
                .project
                .as_ref()
                .and_then(|p| p.json["defs"][kind][0]["uid"].as_i64());
            if kind == "layers" {
                self.select_layer_definition(first);
            } else {
                self.definition_uid = first;
            }
            self.clipboard_focus(
                ctx,
                ClipTarget::Definition {
                    kind: kind.to_owned(),
                    uid: first,
                },
            );
        }
    }

    pub(in crate::app) fn definition_removal_open(ctx: &egui::Context) -> bool {
        ctx.data(|data| {
            data.get_temp::<DefinitionRemoval>(definition_removal_id())
                .is_some()
        })
    }

    pub(in crate::app) fn definition_removal_dialog(&mut self, ctx: &egui::Context) {
        let id = definition_removal_id();
        let Some(pending) = ctx.data(|data| data.get_temp::<DefinitionRemoval>(id)) else {
            return;
        };
        if self
            .project
            .as_ref()
            .is_none_or(|p| p.is_backup() || p.json["iid"] != pending.project_iid)
        {
            ctx.data_mut(|data| data.remove::<DefinitionRemoval>(id));
            return;
        }
        let modal = project_dialogs::form_warning_dialog(id)
            .area(egui::Modal::default_area(id))
            .frame(
                project_dialogs::dialog_frame(20)
                    .fill(Color32::from_rgb(121, 17, 5))
                    .stroke(Stroke::new(1_f32, Color32::from_rgb(121, 17, 5))),
            )
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 42.).clamp(1., 358.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                let warning = locale::tr(ui, pending.warning);
                for (index, paragraph) in warning.lines().enumerate() {
                    if index > 0 {
                        ui.add_space(14.);
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(paragraph).size(14.667).color(Color32::WHITE),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    let size = egui::vec2((width - 2.) * 0.5, 35.);
                    let confirm = project_dialogs::button(ui, "CONFIRM", size, false, 13.333);
                    let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333);
                    (confirm.clicked(), cancel.clicked())
                })
                .inner
            });
        let (confirm, cancel) = modal.inner;
        let confirm = confirm
            || modal.is_top_modal
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        let cancel = cancel || modal.should_close();
        if confirm || cancel {
            ctx.data_mut(|data| data.remove::<DefinitionRemoval>(id));
        }
        if confirm && !cancel {
            self.remove_definition_from_editor(ctx, &pending.kind, pending.uid);
        }
    }

    fn clipboard_delete(&mut self, ctx: &egui::Context, target: &ClipTarget) {
        match target {
            ClipTarget::Definition {
                kind,
                uid: Some(uid),
            } => {
                self.request_definition_removal(ctx, kind, *uid);
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
                if self.last_rule_list_row == Some(*rule) {
                    self.last_rule_list_row = None;
                }
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
            ClipTarget::Definition { uid, .. } => {
                if !target.accepts_clipboard(data.kind) {
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
                if !target.accepts_clipboard(data.kind) {
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
                ClipTarget::Definition { kind, .. } if kind == "layers" => {
                    self.select_layer_definition(Some(uid));
                }
                ClipTarget::Definition { .. } => self.definition_uid = Some(uid),
                ClipTarget::Field { .. } => self.field_uid = Some(uid),
                ClipTarget::Rule { group, .. } => {
                    if data.kind == ClipboardKind::RuleGroup {
                        self.rule_group_uid = Some(uid);
                        self.rule_uid = None;
                    } else {
                        self.rule_group_uid = Some(*group);
                        self.rule_uid = Some(uid);
                        self.last_rule_list_row = Some(uid);
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
    pub(super) fn definition_list_paste_menu(&mut self, ui: &mut egui::Ui, target: ClipTarget) {
        let mut background = ui.available_rect_before_wrap();
        background.max.y = ui.clip_rect().max.y;
        if !background.is_positive() {
            return;
        }
        let response = ui.interact(
            background,
            ui.id().with("definition-list-background"),
            Sense::click(),
        );
        let menu = egui::Popup::context_menu(&response)
            .gap(10.)
            .frame(source_controls::context_menu_frame(ui.style()))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let paste =
                    clipboard_preview(ui).filter(|data| target.accepts_clipboard(data.kind));
                let label = locale::tr(ui, "Paste").to_uppercase();
                let enabled = paste.is_some();
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(81.33, 31.),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label)
                });
                let hovered = enabled && response.hovered();
                let ink = if hovered {
                    ui.painter().rect_filled(rect, 0., theme::ACCENT);
                    Color32::BLACK
                } else if enabled {
                    theme::ACCENT
                } else {
                    source_controls::disabled_hatch(ui, rect);
                    theme::LIGHT
                };
                ui.painter().text(
                    rect.left_center() + egui::vec2(20., 0.),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(13.33),
                    ink,
                );
                if response.clicked()
                    && let Some(data) = paste
                {
                    self.clipboard_apply(&target, &data);
                    ui.close();
                }
            });
        if menu.is_some() {
            let clip = ui.clip_rect().shrink(0.5);
            ui.painter().extend(egui::Shape::dashed_line(
                &[
                    clip.left_top(),
                    clip.right_top(),
                    clip.right_bottom(),
                    clip.left_bottom(),
                    clip.left_top(),
                ],
                Stroke::new(1_f32, theme::ACCENT),
                3.,
                2.,
            ));
        }
    }

    pub(super) fn rule_list_paste_menu(&mut self, ui: &mut egui::Ui, layer: i64) {
        let paste = clipboard_preview(ui).filter(|data| data.kind == ClipboardKind::RuleGroup);
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

    pub(super) fn clipboard_menu(&mut self, ui: &mut egui::Ui, target: ClipTarget) {
        enum Action {
            Copy,
            Cut,
            Paste,
            Duplicate,
            Delete,
        }

        let paste = clipboard_preview(ui).filter(|data| target.accepts_clipboard(data.kind));
        let can_copy = !self.clipboard_is_embedded_tileset(&target);
        let (width, duplicate_width, height) = match &target {
            ClipTarget::Rule { rule: None, .. } => (78., 78., 38.),
            ClipTarget::Rule { .. } => (46., 46., 33.),
            _ => (50., 41.45, 38.),
        };
        let definition_menu = !matches!(target, ClipTarget::Rule { .. });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            let buttons = [
                (
                    Action::Copy,
                    Some(egui::include_image!("../../assets/icons/copy.svg")),
                    "Copy",
                ),
                (
                    Action::Cut,
                    Some(egui::include_image!("../../assets/icons/cut.svg")),
                    "Cut",
                ),
                (
                    Action::Paste,
                    Some(egui::include_image!("../../assets/icons/paste.svg")),
                    "Paste",
                ),
                (Action::Duplicate, None, "Duplicate"),
                (
                    Action::Delete,
                    Some(egui::include_image!("../../assets/icons/delete.svg")),
                    "Delete",
                ),
            ];
            for (action, icon, label) in buttons {
                let enabled = match action {
                    Action::Copy | Action::Cut | Action::Duplicate => can_copy,
                    Action::Paste => paste.is_some(),
                    Action::Delete => true,
                };
                let width = if matches!(action, Action::Duplicate) {
                    duplicate_width
                } else {
                    width
                };
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(width, height),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                let hover = enabled && response.hovered();
                if hover {
                    ui.painter().rect_filled(rect, 0., theme::ACCENT);
                } else if !enabled {
                    source_controls::disabled_hatch(ui, rect);
                }
                if let Some(icon) = icon {
                    let ink = if hover {
                        Color32::BLACK
                    } else {
                        Color32::from_white_alpha(if enabled { 179 } else { 64 })
                    };
                    let inset = if definition_menu { 4. } else { 0. };
                    egui::Image::new(icon).tint(ink).paint_at(
                        ui,
                        Rect::from_center_size(
                            rect.center() - egui::vec2(inset, 0.),
                            Vec2::splat(22.),
                        ),
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "x2",
                        FontId::proportional(16.),
                        if hover {
                            Color32::BLACK
                        } else if enabled {
                            theme::TEXT
                        } else {
                            theme::LIGHT
                        },
                    );
                }
                if response.action_tooltip(locale::tr(ui, label)).clicked() {
                    match action {
                        Action::Copy => self.clipboard_copy(ui.ctx(), &target, false),
                        Action::Cut => self.clipboard_copy(ui.ctx(), &target, true),
                        Action::Paste => {
                            if let Some(data) = &paste {
                                self.clipboard_apply(&target, data);
                            }
                        }
                        Action::Duplicate => self.clipboard_duplicate(&target),
                        Action::Delete => self.clipboard_delete(ui.ctx(), &target),
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
                    self.clipboard_paste(&raw, &target);
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
    fn cutting_a_linked_intgrid_copies_before_changing_the_project() {
        let ctx = egui::Context::default();
        let mut project = Project::new();
        let source = project.add_layer("Terrain", "IntGrid", 16).unwrap();
        let auto = project.add_layer("Walls", "AutoLayer", 16).unwrap();
        project
            .update_layer_properties(
                auto,
                &serde_json::json!({
                    "autoSourceLayerDefUid": source
                }),
            )
            .unwrap();
        let before = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let target = ClipTarget::Definition {
            kind: "layers".into(),
            uid: Some(source),
        };
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            app.clipboard_copy(ctx, &target, true);
        });
        assert!(output.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(raw)
                if ClipboardData::decode(raw).is_ok_and(|item| item.kind == ClipboardKind::LayerDef))
        }));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(app.last_chance.is_none());
        let key_event = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let render = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.definition_removal_dialog(ctx);
                },
            );
        };
        render(&mut app, vec![]);
        render(&mut app, vec![key_event(Key::Escape)]);
        assert!(!EditorApp::definition_removal_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().json, before);

        app.request_definition_removal(&ctx, "layers", source);
        render(&mut app, vec![]);
        render(&mut app, vec![key_event(Key::Enter)]);
        assert!(!EditorApp::definition_removal_open(&ctx));
        let removed = app.project.as_ref().unwrap();
        assert!(removed.layer_def(source).is_none());
        assert!(removed.layer_def(auto).unwrap()["autoSourceLayerDefUid"].is_null());
        app.restore_last_chance();
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

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
    fn used_entity_and_enum_definitions_require_the_source_warnings() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let used = project.add_entity_def("Used", 16, 16).unwrap();
        let spare = project.add_entity_def("Spare", 16, 16).unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, used, 16, 16).unwrap();
        assert!(definition_removal_warning(&project, "entities", used).is_some());
        assert!(definition_removal_warning(&project, "entities", spare).is_none());

        let entity_enum = project
            .add_enum_definition("Direction", &["North"])
            .unwrap();
        let level_enum = project.add_enum_definition("Biome", &["Forest"]).unwrap();
        let unused_enum = project.add_enum_definition("Unused", &["Value"]).unwrap();
        project
            .add_field_definition(Some(spare), "Facing", "LocalEnum.Direction", true)
            .unwrap();
        project
            .add_field_definition(None, "Region", "LocalEnum.Biome", false)
            .unwrap();
        assert!(definition_removal_warning(&project, "enums", entity_enum).is_some());
        assert!(definition_removal_warning(&project, "enums", level_enum).is_some());
        assert!(definition_removal_warning(&project, "enums", unused_enum).is_none());
    }

    #[test]
    fn definition_menu_requests_clipboard_without_pasting() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let result = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.clipboard_menu(
                    ui,
                    ClipTarget::Definition {
                        kind: "layers".into(),
                        uid: Some(1),
                    },
                );
            });
        });
        assert!(
            result.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::RequestPaste))
        );
        assert!(!app.dirty);
    }

    #[test]
    fn rule_menu_preview_consumes_only_the_queued_clipboard_read() {
        let ctx = egui::Context::default();
        let app = EditorApp::empty();
        let show_menu = |ctx: &egui::Context, expected| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert_eq!(clipboard_preview(ui).map(|data| data.kind), expected);
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
        app.capture_clipboard_preview(&ctx, &mut input);
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
