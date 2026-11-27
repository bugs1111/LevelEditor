use super::*;
#[derive(Clone)]
struct PolicyDialog {
    layer: i64,
    group: i64,
    value: Option<Option<i32>>,
}
fn dialog_id() -> egui::Id {
    egui::Id::new("source-group-out-of-bounds")
}
impl EditorApp {
    pub(super) fn begin_group_policy(&self, ctx: &egui::Context, layer: i64, group: i64) {
        ctx.data_mut(|d| {
            d.insert_temp(
                dialog_id(),
                PolicyDialog {
                    layer,
                    group,
                    value: None,
                },
            )
        });
    }
    pub(super) fn group_policy_dialog(&mut self, ctx: &egui::Context) {
        let id = dialog_id();
        let Some(mut state) = ctx.data_mut(|d| d.get_temp::<PolicyDialog>(id)) else {
            return;
        };
        let Some(layer) = self.project.as_ref().and_then(|p| p.layer_def(state.layer)) else {
            ctx.data_mut(|d| d.remove::<PolicyDialog>(id));
            return;
        };
        let source = layer["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(layer);
        let values = source["intGridValues"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut apply = false;
        let mut cancel = false;
        let modal = egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(153))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin { left: 20, right: 20, top: 20, bottom: 23 })
                    .show(ui, |ui| {
                        let width = (ctx.content_rect().width() - 60.).clamp(240., 600.);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 0.;
                        source_controls::help_card(ui, width, 133., |ui| {
                            ui.spacing_mut().item_spacing.y = 8.;
                            for paragraph in [
                                "You can overwrite the \"out-of-bounds policy\" of all rules in this group in one step.",
                                "The out-of-bounds policy defines which IntGrid value should be returned when the rule evaluate cells outside of current layer bounds.",
                                "The default setting prevents rules from working on level edges if they are checking for layer values outside of level bounds.",
                            ] {
                                source_controls::help_paragraph(ui, paragraph);
                            }
                        });
                        ui.add_space(16.);
                        source_controls::form_row_wrapped_sized(ui, "\"Out-of-bounds\" policy", "outOfBoundsValue", egui::vec2(110., 34.), |ui| {
                            let mut choices = vec![
                                (None, locale::tr(ui, "-- Pick a value --").to_owned()),
                                (Some(None), locale::tr(ui, "This rule should not apply when reading cells outside of layer bounds (default)").to_owned()),
                                (Some(Some(0)), locale::tr(ui, "Empty cells").to_owned()),
                            ];
                            choices.extend(values.iter().filter_map(|value| {
                                let id = i32::try_from(value["value"].as_i64()?).ok()?;
                                let label = value["identifier"].as_str().map_or_else(
                                    || id.to_string(), |name| format!("{id} - {name}"),
                                );
                                Some((Some(Some(id)), label))
                            }));
                            let selected = choices.iter().position(|(value, _)| *value == state.value).unwrap_or(0);
                            let options: Vec<_> = choices.iter().map(|(_, label)| (label.as_str(), None)).collect();
                            if let Some(picked) = source_controls::form_select(
                                ui, id.with("value"), 29., &options, Some(selected), &choices[selected].1, source_controls::FormSelectAppearance::Normal,
                            ) {
                                state.value = choices[picked].0;
                            }
                        });
                        ui.add_space(43.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            ui.add_space(1.);
                            let size = egui::vec2((width - 4.) * 0.5, 35.);
                            apply = project_dialogs::button(ui, "APPLY TO ALL RULES", size, false, 13.333).clicked();
                            cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                        });
                    });
            });
        let mut close = cancel || modal.should_close();
        if apply && let Some(value) = state.value {
            close = self
                .edit("Group out-of-bounds policy changed", |p| {
                    p.set_rule_group_out_of_bounds(state.layer, state.group, value)
                })
                .is_some();
        }
        if close {
            ctx.data_mut(|d| d.remove::<PolicyDialog>(id));
        } else {
            ctx.data_mut(|d| d.insert_temp(id, state));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_policy_apply_button_is_one_undoable_transaction() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut p = Project::new();
        let layer = p.add_layer("Terrain", "IntGrid", 16).unwrap();
        let group = p.add_auto_rule_group(layer, "Walls").unwrap();
        let a = p.add_auto_rule(layer, group, 1).unwrap();
        let b = p.add_auto_rule(layer, group, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.begin_group_policy(&ctx, layer, group);
        let before = app.project.as_ref().unwrap().json.clone();
        let run = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.group_policy_dialog(ctx),
            )
        };
        run(&mut app, vec![]);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        run(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(ctx.data(|d| d.get_temp::<PolicyDialog>(dialog_id()).is_none()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        app.begin_group_policy(&ctx, layer, group);
        run(&mut app, vec![]);
        for name in ["-- Pick a value --", "Empty cells", "Apply to all rules"] {
            let out = run(&mut app, vec![]);
            if name == "-- Pick a value --" {
                assert!(out.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text)
                        if text.galley.text().replace('\n', " ") == "\"Out-of-bounds\" policy"
                            && text.galley.job.sections[0].format.font_id == theme::TextRole::PropertyLabel.font()
                )));
            }
            if name == "Empty cells" {
                let (clip, label) = out
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text().starts_with("This rule should not apply") =>
                        {
                            Some((shape.clip_rect, text.visual_bounding_rect()))
                        }
                        _ => None,
                    })
                    .expect("visible default policy option");
                assert!(
                    label.right() <= clip.right() + 1.,
                    "default option clipped: {label:?} in {clip:?}"
                );
            }
            let point = out
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text().eq_ignore_ascii_case(name) => {
                        Some(t.pos + t.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {name}"));
            for pressed in [true, false] {
                run(
                    &mut app,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
        }
        for uid in [a, b] {
            let copy = app
                .project
                .as_ref()
                .unwrap()
                .copy_auto_rule(layer, group, Some(uid))
                .unwrap();
            assert_eq!(copy.value["outOfBoundsValue"], 0);
        }
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.history.undo(app.project.as_mut().unwrap()));
    }
}
