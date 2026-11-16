use super::*;
#[derive(Clone)]
struct TransferDialog {
    source: i64,
    target: Option<i64>,
    filter: String,
    result: Option<String>,
}
impl EditorApp {
    pub(super) fn begin_entity_transfer(&self, ctx: &egui::Context, source: i64) {
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("move-entities-between-layers"),
                TransferDialog {
                    source,
                    target: None,
                    filter: String::new(),
                    result: None,
                },
            )
        });
    }
    pub(super) fn entity_transfer_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("move-entities-between-layers");
        let Some(mut state) = ctx.data_mut(|d| d.get_temp::<TransferDialog>(id)) else {
            return;
        };
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let entities = p.json["defs"]["entities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let layers = p.json["defs"]["layers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut close = false;
        let mut apply = false;
        egui::Modal::new(id).show(ctx, |ui| {
            ui.heading("Batch move existing entity instances");
            ui.label("Move matching instances from the origin layer in every world and level.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Move all instances");
                egui::ComboBox::from_id_salt((id, "filter"))
                    .width(200.)
                    .selected_text(if state.filter.is_empty() {
                        "Pick entities"
                    } else {
                        &state.filter
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.filter, "all.*".into(), "All entities");
                        ui.separator();
                        ui.label("Entity identifiers");
                        for d in &entities {
                            let name = text(d, "identifier", "");
                            ui.selectable_value(&mut state.filter, format!("id.{name}"), name);
                        }
                        ui.separator();
                        ui.label("With tag");
                        let tags = entities
                            .iter()
                            .flat_map(|d| d["tags"].as_array().into_iter().flatten())
                            .filter_map(Value::as_str)
                            .collect::<std::collections::BTreeSet<_>>();
                        for tag in tags {
                            ui.selectable_value(&mut state.filter, format!("tag.{tag}"), tag);
                        }
                    });
                ui.label("to layer");
                egui::ComboBox::from_id_salt((id, "layer"))
                    .width(180.)
                    .selected_text(
                        layers
                            .iter()
                            .find(|d| d["uid"].as_i64() == state.target)
                            .map(|d| text(d, "identifier", "Layer"))
                            .unwrap_or("Pick a layer"),
                    )
                    .show_ui(ui, |ui| {
                        for d in &layers {
                            if let Some(uid) = d["uid"].as_i64() {
                                let enabled = d["type"] == "Entities" && uid != state.source;
                                ui.add_enabled_ui(enabled, |ui| {
                                    ui.selectable_value(
                                        &mut state.target,
                                        Some(uid),
                                        text(d, "identifier", "Layer"),
                                    );
                                });
                            }
                        }
                    });
            });
            let preview = state
                .target
                .map(|to| p.count_entities_to_transfer(state.source, to, &state.filter));
            match &preview {
                Some(Ok(n)) => {
                    ui.label(format!("{n} matching entities"));
                }
                Some(Err(error)) => {
                    ui.colored_label(theme::ACCENT, error.to_string());
                }
                None => {
                    ui.label("Choose the entities and destination layer first.");
                }
            }
            if let Some(result) = &state.result {
                ui.label(result);
            }
            ui.horizontal(|ui| {
                if source_controls::add_enabled_button(
                    ui,
                    matches!(preview,Some(Ok(n))if n>0),
                    egui::Button::new("Move entities"),
                )
                .clicked()
                {
                    apply = true;
                }
                if source_controls::button(ui, "Close").clicked() {
                    close = true;
                }
            });
        });
        if apply
            && let Some(target) = state.target
            && let Some(count) = self.edit_recoverable("Moved entities between layers", |p| {
                p.move_entities_between_layers(state.source, target, &state.filter)
            })
        {
            state.result = Some(format!("{count} entities were moved."));
            self.entity_index = None;
        }
        if close {
            ctx.data_mut(|d| d.remove::<TransferDialog>(id));
        } else {
            ctx.data_mut(|d| d.insert_temp(id, state));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.entity_transfer_dialog(ctx),
        )
    }
    fn center(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.job.text == label => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| center(s, label)),
            _ => None,
        }
    }
    #[test]
    fn native_batch_move_button_moves_instances_and_offers_last_chance_restore() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let from = p.add_layer("Origin", "Entities", 16).unwrap();
        let to = p.add_layer("Destination", "Entities", 16).unwrap();
        let entity = p.add_entity_def("Actor", 16, 16).unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 1, entity, 16, 16).unwrap();
        let before = p.json.clone();
        app.set_project(p);
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new("move-entities-between-layers"),
                TransferDialog {
                    source: from,
                    target: Some(to),
                    filter: "all.*".into(),
                    result: None,
                },
            )
        });
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let point = output
            .shapes
            .iter()
            .find_map(|s| center(&s.shape, "Move entities"))
            .unwrap();
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
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
        assert!(
            app.project.as_ref().unwrap().layer(key, 1).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(app.last_chance.is_some());
        app.restore_last_chance();
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
}
