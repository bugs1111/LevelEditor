//! Shared editor for LDtk definition tags.
use super::*;
use crate::app::chrome::ActionTooltip as _;

#[derive(Clone, Copy)]
pub(super) enum Field<'a> {
    Definition(&'a str),
    LayerUi,
    RequiredEntities,
    ExcludedEntities,
    ReferenceTargets,
}

impl Field<'_> {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Definition(_) => "tags",
            Self::LayerUi => "uiFilterTags",
            Self::RequiredEntities => "requiredTags",
            Self::ExcludedEntities => "excludedTags",
            Self::ReferenceTargets => "allowedRefTags",
        }
    }
}

#[derive(Clone)]
struct Draft {
    old: Option<String>,
    value: String,
    focus: bool,
}

#[derive(Clone)]
struct Rename {
    old: String,
    new: String,
}

pub(super) enum Change {
    Set(Value),
    Rename {
        old: String,
        new: String,
        everywhere: bool,
    },
}

fn tag_action(
    ui: &mut egui::Ui,
    id: egui::Id,
    icon: egui::ImageSource<'static>,
    label: &str,
    size: Vec2,
    framed: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    if framed {
        let fill = if response.hovered() {
            Color32::from_rgb(59, 65, 81)
        } else {
            Color32::from_rgb(47, 53, 67)
        };
        ui.painter().rect_filled(
            rect.translate(egui::vec2(0., 2.)),
            1.,
            Color32::from_black_alpha(98),
        );
        ui.painter().rect(
            rect,
            1.,
            fill,
            Stroke::new(1_f32, Color32::from_rgb(30, 34, 43)),
            StrokeKind::Inside,
        );
    }
    let icon_size = if framed { 22. } else { 16. };
    egui::Image::new(icon).paint_at(
        ui,
        Rect::from_center_size(rect.center(), Vec2::splat(icon_size)),
    );
    let label = locale::tr(ui, label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .action_tooltip(label)
}

pub(super) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    field: Field<'_>,
    uid: i64,
    value: &Value,
) -> Option<Change> {
    let layer_filter = matches!(field, Field::RequiredEntities | Field::ExcludedEntities);
    let is_filter = layer_filter || matches!(field, Field::ReferenceTargets);
    let kind = match field {
        Field::Definition(kind) => kind,
        Field::LayerUi => "layers",
        Field::RequiredEntities | Field::ExcludedEntities | Field::ReferenceTargets => "entities",
    };
    let key = if is_filter { "tags" } else { field.key() };
    let id = egui::Id::new((
        "definition-tags",
        text(&project.json, "iid", ""),
        kind,
        field.key(),
        uid,
    ));
    let draft_id = id.with("draft");
    let rename_id = id.with("rename");
    let mut draft = ui.ctx().data(|data| data.get_temp::<Draft>(draft_id));
    let mut changed = None;
    let mut tags = value.as_array().cloned().unwrap_or_default();
    let definitions = project.json["defs"][kind]
        .as_array()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut recalled: Vec<_> = definitions
        .iter()
        .flat_map(|def| def[key].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    if kind == "entities" && !matches!(field, Field::ReferenceTargets) {
        for layer in project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
        {
            for tag in ["requiredTags", "excludedTags"]
                .into_iter()
                .flat_map(|key| layer[key].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
            {
                recalled.push(tag);
            }
        }
        let current_layer = layer_filter.then(|| project.layer_def(uid)).flatten();
        recalled.retain(|tag| {
            if let Some(layer) = current_layer {
                !["requiredTags", "excludedTags"].into_iter().any(|key| {
                    layer[key]
                        .as_array()
                        .is_some_and(|tags| tags.iter().any(|v| v == *tag))
                })
            } else {
                !tags.iter().any(|v| v == *tag)
            }
        });
    } else {
        recalled.sort_by_cached_key(|tag| tag.to_lowercase());
        if is_filter {
            recalled.retain(|tag| !tags.iter().any(|value| value == *tag));
        }
    }
    let mut unique = std::collections::HashSet::new();
    recalled.retain(|tag| unique.insert(*tag));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2., 2.);
        let mut empty_recall = None;
        if tags.is_empty() && draft.is_none() {
            let response = ui
                .add_sized(
                    [65., 24.],
                    egui::Button::new(
                        RichText::new(locale::tr(ui, "(No tag)"))
                            .size(12.)
                            .italics()
                            .color(Color32::from_rgb(115, 127, 157)),
                    )
                    .frame(false),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let rect = response.rect.shrink(0.5);
            ui.painter().extend(egui::Shape::dashed_line(
                &[
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ],
                Stroke::new(1_f32, Color32::from_rgb(84, 93, 115)),
                2.,
                2.,
            ));
            ui.add_space(14.);
            if is_filter && !recalled.is_empty() {
                empty_recall = Some(response);
            } else if !is_filter && response.clicked() {
                draft = Some(Draft {
                    old: None,
                    value: String::new(),
                    focus: true,
                });
            }
        }
        let edit_input = |ui: &mut egui::Ui,
                          draft: &mut Draft,
                          tags: &mut Vec<Value>,
                          changed: &mut Option<Change>| {
            ui_commit::prepare_editor(ui.ctx(), draft_id);
            let response = ui.add_sized(
                [80., 27.],
                egui::TextEdit::singleline(&mut draft.value).id(draft_id),
            );
            ui_commit::track_editor(ui.ctx(), &response);
            if draft.focus {
                response.request_focus();
                draft.focus = false;
            }
            let escape = response.has_focus() && ui.input(|i| i.key_pressed(Key::Escape));
            let commit = response.lost_focus()
                || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if escape {
                return true;
            }
            if commit {
                if let Some(next) = Project::cleanup_tag(&draft.value) {
                    if let Some(old) = draft.old.as_ref() {
                        if *old != next {
                            let uses = definitions
                                .iter()
                                .filter(|d| {
                                    d[key]
                                        .as_array()
                                        .is_some_and(|a| a.iter().any(|v| v == old))
                                })
                                .count();
                            if uses > 1 {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        rename_id,
                                        Rename {
                                            old: old.clone(),
                                            new: next,
                                        },
                                    )
                                });
                            } else {
                                *changed = Some(Change::Rename {
                                    old: old.clone(),
                                    new: next,
                                    everywhere: true,
                                });
                            }
                        }
                    } else if !tags.iter().any(|v| v == &next) {
                        tags.push(json!(next));
                        *changed = Some(Change::Set(json!(tags)));
                    }
                }
                return true;
            }
            false
        };
        let mut done = false;
        for (index, tag) in tags.clone().iter().enumerate() {
            let name = tag.as_str().unwrap_or("");
            if draft
                .as_ref()
                .is_some_and(|d| d.old.as_deref() == Some(name))
            {
                done = edit_input(ui, draft.as_mut().unwrap(), &mut tags, &mut changed);
                continue;
            }
            egui::Frame::new()
                .fill(Color32::from_rgb(161, 114, 80))
                .stroke(Stroke::new(1_f32, Color32::from_rgb(197, 146, 109)))
                .corner_radius(egui::CornerRadius {
                    nw: 2,
                    sw: 2,
                    ne: 8,
                    se: 8,
                })
                .inner_margin(egui::Margin {
                    left: 4,
                    right: 7,
                    top: 2,
                    bottom: 2,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(name)
                                        .size(14.667)
                                        .italics()
                                        .color(Color32::WHITE),
                                )
                                .sense(if is_filter {
                                    Sense::hover()
                                } else {
                                    Sense::click()
                                }),
                            )
                            .clicked()
                        {
                            draft = Some(Draft {
                                old: Some(name.into()),
                                value: name.into(),
                                focus: true,
                            });
                        }
                        if tag_action(
                            ui,
                            id.with(("remove", index)),
                            egui::include_image!("../../assets/icons/clear.svg"),
                            "Remove tag",
                            egui::vec2(18., 21.),
                            false,
                        )
                        .clicked()
                        {
                            tags.remove(index);
                            changed = Some(Change::Set(json!(tags)));
                        }
                    });
                });
        }
        if let Some(input) = draft.as_mut().filter(|d| d.old.is_none()) {
            done = edit_input(ui, input, &mut tags, &mut changed);
        }
        if done {
            draft = None;
        }
        if !is_filter
            && tag_action(
                ui,
                id.with("add"),
                egui::include_image!("../../assets/icons/add.svg"),
                "Add tag",
                egui::vec2(32., 30.),
                true,
            )
            .clicked()
        {
            draft = Some(Draft {
                old: None,
                value: String::new(),
                focus: true,
            });
        }
        if !recalled.is_empty() {
            let recall = tag_action(
                ui,
                id.with("recall"),
                egui::include_image!("../../assets/icons/recall.svg"),
                "Recall tag",
                egui::vec2(32., 30.),
                true,
            );
            for response in empty_recall.iter().chain(std::iter::once(&recall)) {
                egui::Popup::menu(response)
                    .align(egui::RectAlign::TOP_START)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::DARK)
                            .stroke(Stroke::new(1_f32, theme::LIGHT))
                            .corner_radius(2.)
                            .shadow(egui::Shadow {
                                offset: [0, 4],
                                blur: 12,
                                spread: 0,
                                color: Color32::from_black_alpha(210),
                            }),
                    )
                    .show(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        let font = FontId::proportional(13.333);
                        let options: Vec<_> = recalled
                            .iter()
                            .map(|tag| {
                                ui.painter().layout_no_wrap(
                                    tag.to_uppercase(),
                                    font.clone(),
                                    theme::ACCENT,
                                )
                            })
                            .collect();
                        let width = options
                            .iter()
                            .map(|label| label.size().x)
                            .fold(0., f32::max)
                            + 40.;
                        for (tag, label) in recalled.iter().zip(options) {
                            let (rect, response) =
                                ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
                            if response.hovered() || response.has_focus() {
                                ui.painter().rect_filled(rect, 0., theme::ACCENT);
                            }
                            ui.painter().galley_with_override_text_color(
                                rect.left_center() + egui::vec2(20., -label.size().y / 2.),
                                label.clone(),
                                if response.hovered() || response.has_focus() {
                                    Color32::BLACK
                                } else {
                                    theme::ACCENT
                                },
                            );
                            ui.painter().hline(
                                rect.x_range(),
                                rect.bottom(),
                                Stroke::new(1_f32, theme::MEDIUM),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    label.text(),
                                )
                            });
                            if response.clicked() {
                                if !tags.iter().any(|v| v == *tag) {
                                    tags.push(json!(tag));
                                    changed = Some(Change::Set(json!(tags)));
                                }
                                ui.close();
                            }
                        }
                    });
            }
        }
    });
    ui.ctx().data_mut(|data| {
        if let Some(draft) = draft {
            data.insert_temp(draft_id, draft);
        } else {
            data.remove::<Draft>(draft_id);
        }
    });
    if let Some(rename) = ui.ctx().data(|data| data.get_temp::<Rename>(rename_id)) {
        use crate::app::project_dialogs::{button, dialog_frame};
        let mut choice = None;
        let mut cancel = false;
        let modal = egui::Modal::new(rename_id)
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ui.ctx(), |ui| {
                let width = (ui.ctx().content_rect().width() - 40.).clamp(180., 360.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 2.);
                ui.add_space(14.);
                let message = locale::tr(ui, "This tag is used in other elements!\nDo you want to rename it in ALL other elements as well, or only here?");
                ui.label(RichText::new(message.replace('\n', "\n\n")).size(14.667).color(Color32::WHITE));
                ui.add_space(13.);
                ui.horizontal(|ui| {
                    let size = egui::vec2((width - 2.) / 2., 35.);
                    for (label, everywhere) in [("Rename everywhere", true), ("Rename only here", false)] {
                        if button(ui, label, size, false, 13.333).clicked() {
                            choice = Some(everywhere);
                        }
                    }
                });
                cancel = button(ui, "Cancel", egui::vec2(width, 35.), true, 13.333).clicked();
                ui.add_space(3.);
            });
        if cancel || choice.is_some() || modal.should_close() {
            ui.ctx().data_mut(|data| data.remove::<Rename>(rename_id));
        }
        if let Some(everywhere) = choice {
            changed = Some(Change::Rename {
                old: rename.old,
                new: rename.new,
                everywhere,
            });
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        p: &mut Project,
        uid: i64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut change = None;
        let tags = p.json["defs"]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["uid"] == uid)
            .unwrap()["tags"]
            .clone();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700., 500.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    change = show(ui, p, Field::Definition("entities"), uid, &tags);
                });
            },
        );
        if let Some(change) = change {
            match change {
                Change::Set(value) => p
                    .update_entity_definition(uid, &json!({"tags":value}))
                    .unwrap(),
                Change::Rename {
                    old,
                    new,
                    everywhere,
                } => p
                    .rename_definition_tag("entities", uid, &old, &new, everywhere)
                    .unwrap(),
            }
        }
        output
    }
    fn label(shape: &egui::Shape, expected: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == expected => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, expected)),
            _ => None,
        }
    }
    fn click(ctx: &egui::Context, p: &mut Project, uid: i64, text: &str) {
        let output = frame(ctx, p, uid, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, text))
            .unwrap_or_else(|| panic!("missing {text}"));
        click_at(ctx, p, uid, pos);
    }
    fn click_action(ctx: &egui::Context, p: &mut Project, uid: i64, action: impl std::hash::Hash) {
        frame(ctx, p, uid, vec![]);
        let id = egui::Id::new((
            "definition-tags",
            text(&p.json, "iid", ""),
            "entities",
            "tags",
            uid,
        ));
        let pos = ctx.read_response(id.with(action)).unwrap().rect.center();
        click_at(ctx, p, uid, pos);
    }
    fn click_at(ctx: &egui::Context, p: &mut Project, uid: i64, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                ctx,
                p,
                uid,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(ctx, p, uid, vec![]);
    }
    fn input(ctx: &egui::Context, p: &mut Project, uid: i64, value: &str) {
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(
            ctx,
            p,
            uid,
            vec![key(
                Key::A,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            )],
        );
        frame(
            ctx,
            p,
            uid,
            vec![
                egui::Event::Text(value.into()),
                key(Key::Enter, Default::default()),
            ],
        );
        frame(ctx, p, uid, vec![]);
    }
    #[test]
    fn adding_tag_cleans_input_and_removing_it_updates_definition() {
        let mut p = Project::new();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&ctx, &mut p, uid, "(No tag)");
        input(&ctx, &mut p, uid, " red foe ");
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!(["red_foe"]));
        click_action(&ctx, &mut p, uid, ("remove", 0_usize));
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!([]));
    }
    #[test]
    fn editing_shared_tag_asks_before_renaming_other_definitions() {
        let mut p = Project::new();
        let layer = p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("First", 16, 16).unwrap();
        let other = p.add_entity_def("Second", 16, 16).unwrap();
        p.update_entity_definition(uid, &json!({"tags":["enemy"]}))
            .unwrap();
        p.update_entity_definition(other, &json!({"tags":["enemy"]}))
            .unwrap();
        p.update_layer_properties(layer, &json!({"requiredTags":["enemy"]}))
            .unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&ctx, &mut p, uid, "enemy");
        input(&ctx, &mut p, uid, "foe");
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!(["enemy"]));
        click(&ctx, &mut p, uid, "Rename only here");
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!(["foe"]));
        assert_eq!(p.json["defs"]["entities"][1]["tags"], json!(["enemy"]));
        assert_eq!(
            p.layer_def(layer).unwrap()["requiredTags"],
            json!(["enemy"])
        );
        click(&ctx, &mut p, uid, "foe");
        input(&ctx, &mut p, uid, "enemy");
        click(&ctx, &mut p, uid, "enemy");
        input(&ctx, &mut p, uid, "foe");
        click(&ctx, &mut p, uid, "Rename everywhere");
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!(["foe"]));
        assert_eq!(p.json["defs"]["entities"][1]["tags"], json!(["foe"]));
        assert_eq!(p.layer_def(layer).unwrap()["requiredTags"], json!(["foe"]));
    }
    #[test]
    fn recall_offers_tags_from_other_entities_and_layer_filters() {
        let mut p = Project::new();
        let layer = p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("First", 16, 16).unwrap();
        let other = p.add_entity_def("Second", 16, 16).unwrap();
        p.update_entity_definition(other, &json!({"tags":["boss"]}))
            .unwrap();
        p.update_layer_properties(layer, &json!({"requiredTags":["enemy"]}))
            .unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click_action(&ctx, &mut p, uid, "recall");
        click(&ctx, &mut p, uid, "ENEMY");
        assert_eq!(p.json["defs"]["entities"][0]["tags"], json!(["enemy"]));
        click_action(&ctx, &mut p, uid, "recall");
        click(&ctx, &mut p, uid, "BOSS");
        assert_eq!(
            p.json["defs"]["entities"][0]["tags"],
            json!(["enemy", "boss"])
        );
    }

    #[test]
    fn local_enum_tags_recall_and_rename_from_the_native_panel() {
        let mut project = Project::new();
        let first = project.add_enum_definition("Terrain", &[]).unwrap();
        let second = project.add_enum_definition("Biome", &[]).unwrap();
        project
            .update_enum_definition(second, &json!({"tags":["shared"]}))
            .unwrap();
        let entity = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(entity, &json!({"tags":["shared","entity_only"]}))
            .unwrap();
        let external = project.json["nextUid"].as_i64().unwrap();
        project.json["defs"]["externalEnums"] = json!([{
            "uid":external,"identifier":"Imported","externalRelPath":"imported.json",
            "tags":["shared","external_only"],"values":[{"id":"Value"}]
        }]);
        project.json["nextUid"] = json!(external + 1);

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        app.definition_uid = Some(first);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        let position = |output: &egui::FullOutput, text| {
            output
                .shapes
                .iter()
                .filter_map(|shape| label(&shape.shape, text))
                .find(|pos| pos.x > 286. && pos.y > 100.)
        };
        let click_pos = |app: &mut EditorApp, pos| {
            for pressed in [true, false] {
                frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            frame(app, vec![]);
        };
        let click = |app: &mut EditorApp, text| {
            let pos =
                position(&frame(app, vec![]), text).unwrap_or_else(|| panic!("missing {text}"));
            click_pos(app, pos);
        };
        let rename = |app: &mut EditorApp, choice| {
            click(app, "shared");
            let key = |key, modifiers| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
            frame(
                app,
                vec![key(
                    Key::A,
                    egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                )],
            );
            frame(
                app,
                vec![
                    egui::Event::Text("world terrain".into()),
                    key(Key::Enter, Default::default()),
                ],
            );
            click(app, choice);
        };
        let tags = |app: &EditorApp, kind: &str, index: usize| {
            app.project.as_ref().unwrap().json["defs"][kind][index]["tags"].clone()
        };

        frame(&mut app, vec![]);
        assert!(position(&frame(&mut app, vec![]), "Tags").is_some());
        let project = app.project.as_ref().unwrap();
        let recall_id = egui::Id::new((
            "definition-tags",
            text(&project.json, "iid", ""),
            "enums",
            "tags",
            first,
        ))
        .with("recall");
        click_pos(
            &mut app,
            ctx.read_response(recall_id).unwrap().rect.center(),
        );
        let recall = frame(&mut app, vec![]);
        assert!(position(&recall, "ENTITY_ONLY").is_none());
        assert!(position(&recall, "EXTERNAL_ONLY").is_none());
        click(&mut app, "SHARED");
        assert_eq!(tags(&app, "enums", 0), json!(["shared"]));

        rename(&mut app, "Rename only here");
        assert_eq!(tags(&app, "enums", 0), json!(["world_terrain"]));
        assert_eq!(tags(&app, "enums", 1), json!(["shared"]));
        assert!(app.history.undo(app.project.as_mut().unwrap()));

        rename(&mut app, "Rename everywhere");
        assert_eq!(tags(&app, "enums", 0), json!(["world_terrain"]));
        assert_eq!(tags(&app, "enums", 1), json!(["world_terrain"]));
        assert_eq!(tags(&app, "entities", 0), json!(["shared", "entity_only"]));
        assert_eq!(
            tags(&app, "externalEnums", 0),
            json!(["shared", "external_only"])
        );

        app.definition_uid = Some(external);
        let external_form = frame(&mut app, vec![]);
        assert!(position(&external_form, "Source file").is_some());
        assert!(position(&external_form, "Tags").is_none());
    }
}
