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
            .cloned()
            .unwrap_or_default();
        let mut apply = false;
        let mut close = false;
        egui::Modal::new(id).show(ctx,|ui|{
            ui.set_width(520.);
            ui.heading("Out-of-bounds policy for all rules");
            ui.label("You can overwrite the out-of-bounds policy of all rules in this group in one step.");
            ui.label("The policy defines which IntGrid value is returned when a rule evaluates cells outside the layer. The default prevents rules from working on level edges when they inspect cells outside the layer.");
            egui::ComboBox::from_id_salt((id,"value")).width(500.).selected_text(match state.value{None=>"-- Pick a value --".to_owned(),Some(None)=>"Do not apply outside layer bounds (default)".into(),Some(Some(0))=>"Empty cells".into(),Some(Some(v))=>values.iter().find(|value|value["value"]==v).map(|value|format!("{v} - {}",text(value,"identifier",""))).unwrap_or_else(||v.to_string())}).show_ui(ui,|ui|{
                ui.selectable_value(&mut state.value,Some(None),"Do not apply outside layer bounds (default)");
                ui.selectable_value(&mut state.value,Some(Some(0)),"Empty cells");
                for value in &values{if let Some(v)=value["value"].as_i64().and_then(|v|i32::try_from(v).ok()){ui.selectable_value(&mut state.value,Some(Some(v)),format!("{v} - {}",text(value,"identifier","")));}}
            });
            ui.horizontal(|ui|{
                apply=source_controls::add_enabled_button(ui,state.value.is_some(),egui::Button::new("Apply to all rules")).clicked();
                close=source_controls::button(ui,"Cancel").clicked();
            });
        });
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
        ctx.data_mut(|d| {
            d.insert_temp(
                dialog_id(),
                PolicyDialog {
                    layer,
                    group,
                    value: Some(Some(0)),
                },
            )
        });
        run(&mut app, vec![]);
        let out = run(&mut app, vec![]);
        let point = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Apply to all rules" => {
                    Some(t.pos + t.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap();
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
