//! Confirmation flow for the World form's destructive actions.
use super::*;

#[derive(Clone)]
pub(super) enum WorldFormAction {
    DuplicateLevel(LevelKey),
    DeleteLevel(LevelKey),
    DeleteWorld(usize),
    Settings { world: Option<usize>, patch: Value },
}
#[derive(Clone)]
struct Pending {
    project_iid: Value,
    action: WorldFormAction,
}
fn dialog_id() -> egui::Id {
    egui::Id::new("world-form-confirm")
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Pending>(dialog_id()).is_some())
}
impl EditorApp {
    pub(super) fn queue_world_form_action(&mut self, ctx: &egui::Context, action: WorldFormAction) {
        if !self.world_form_action_allowed(&action) {
            return;
        }
        if let WorldFormAction::Settings { world, patch } = &action {
            let p = self.project.as_ref().unwrap();
            let current = world
                .and_then(|i| p.json["worlds"].get(i))
                .unwrap_or(&p.json);
            if patch
                .get("worldLayout")
                .is_none_or(|layout| *layout == current["worldLayout"])
                || p.level_keys().len() <= 2
            {
                self.apply_world_form_settings(*world, patch.clone());
                return;
            }
        }
        let pending = Pending {
            project_iid: self.project.as_ref().unwrap().json["iid"].clone(),
            action,
        };
        ctx.data_mut(|data| data.insert_temp(dialog_id(), pending));
    }
    fn world_form_action_allowed(&mut self, action: &WorldFormAction) -> bool {
        let Some(p) = &self.project else {
            return false;
        };
        if p.is_backup() {
            return false;
        }
        match action {
            WorldFormAction::DeleteLevel(key) => {
                if p.level(*key).is_none() {
                    return false;
                }
                if p.level_keys()
                    .iter()
                    .filter(|k| k.world == key.world)
                    .count()
                    <= 1
                {
                    self.status = "You can't delete the last level.".into();
                    return false;
                }
            }
            WorldFormAction::DuplicateLevel(key) => {
                if p.level(*key).is_none() {
                    return false;
                }
            }
            WorldFormAction::Settings { world, .. } => {
                if world.is_some_and(|i| p.json["worlds"].get(i).is_none()) {
                    return false;
                }
            }
            WorldFormAction::DeleteWorld(index) => {
                if p.json["worlds"].get(index).is_none() {
                    return false;
                }
                if p.json["worlds"].as_array().map_or(0, Vec::len) <= 1 {
                    self.status = "You can't delete the last world.".into();
                    return false;
                }
            }
        }
        true
    }
    pub(super) fn world_form_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = ctx.data(|data| data.get_temp::<Pending>(dialog_id())) else {
            return;
        };
        if self
            .project
            .as_ref()
            .is_none_or(|p| p.json["iid"] != pending.project_iid || p.is_backup())
        {
            ctx.data_mut(|data| data.remove::<Pending>(dialog_id()));
            return;
        }
        let prompt = match &pending.action {
            WorldFormAction::Settings { .. } => {
                "Changing this will change ALL the level positions! Please make sure you know what you're doing :)"
            }
            WorldFormAction::DuplicateLevel(_) => "Create a copy of the current level?",
            WorldFormAction::DeleteLevel(_) => "Are you sure you want to delete this level?",
            WorldFormAction::DeleteWorld(_) => "Are you sure you want to delete this world?",
        };
        let modal = if matches!(pending.action, WorldFormAction::Settings { .. }) {
            project_dialogs::form_warning_dialog(dialog_id()).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 42.).clamp(0., 358.));
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(
                    RichText::new(locale::tr(ui, prompt))
                        .size(14.667)
                        .color(Color32::WHITE),
                );
                ui.add_space(16.);
                let actions = egui::Frame::new()
                    .inner_margin(1)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let size = egui::vec2((ui.available_width() - 2.) * 0.5, 35.);
                            let confirm =
                                project_dialogs::button(ui, "CONFIRM", size, false, 13.333);
                            let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333);
                            (confirm.clicked(), cancel.clicked())
                        })
                        .inner
                    })
                    .inner;
                ui.add_space(2.);
                actions
            })
        } else {
            egui::Modal::new(dialog_id()).show(ctx, |ui| {
                ui.label(locale::tr(ui, prompt));
                ui.horizontal(|ui| {
                    let confirm = ui.button(locale::tr(ui, "Confirm")).clicked();
                    let cancel = ui.button(locale::tr(ui, "Cancel")).clicked();
                    (confirm, cancel)
                })
                .inner
            })
        };
        let (mut confirm, mut cancel) = modal.inner;
        confirm |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        cancel |= modal.should_close();
        if confirm || cancel {
            ctx.data_mut(|data| data.remove::<Pending>(dialog_id()));
        }
        if confirm && !cancel && self.world_form_action_allowed(&pending.action) {
            match pending.action {
                WorldFormAction::Settings { world, patch } => {
                    self.apply_world_form_settings(world, patch)
                }
                WorldFormAction::DuplicateLevel(key) => {
                    self.duplicate_context_level(key);
                    self.fit = true;
                }
                WorldFormAction::DeleteLevel(key) => {
                    self.delete_context_level(key);
                    self.world = true;
                }
                WorldFormAction::DeleteWorld(world) => {
                    if self
                        .edit_recoverable("World deleted", |p| p.remove_world(world))
                        .is_some()
                    {
                        // Source selects the first other world before deleting the current one.
                        // After removal that world's index is always zero.
                        self.navigate_world(Some(0));
                    }
                }
            }
        }
    }
    fn apply_world_form_settings(&mut self, world: Option<usize>, mut patch: Value) {
        let layout_changed = patch.get("worldLayout").is_some();
        let operation = move |p: &mut Project| {
            if let Some(identifier) = patch["identifier"].as_str() {
                if world.is_some() {
                    let mut clean = p.cleanup_identifier(identifier);
                    if clean.is_empty() || clean == "_" {
                        clean = p.cleanup_identifier("Unnamed");
                    }
                    patch["identifier"] = serde_json::json!(panels::unique_name(
                        &clean,
                        p.json["worlds"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .filter(|(i, _)| Some(*i) != world)
                            .filter_map(|(_, w)| w["identifier"].as_str())
                    ));
                } else {
                    patch.as_object_mut().unwrap().remove("identifier");
                }
            }
            let current = world
                .and_then(|i| p.json["worlds"].get(i))
                .unwrap_or(&p.json);
            let mut next = current.clone();
            ui_fields::apply_patch(&mut next, &patch);
            let layout = text(&next, "worldLayout", "Free").to_owned();
            let grid_w = number(&next, "worldGridWidth", 256.) as i32;
            let grid_h = number(&next, "worldGridHeight", 256.) as i32;
            for (dimension, grid) in [
                ("defaultLevelWidth", grid_w),
                ("defaultLevelHeight", grid_h),
            ] {
                if let Some(value) = patch.get(dimension).and_then(Value::as_f64) {
                    let value =
                        value.clamp(f64::from(number(&p.json, "defaultGridSize", 16.)), 9999.);
                    let value = if layout == "GridVania" {
                        ((value / f64::from(grid) + 0.5).floor() as i64).max(1) * i64::from(grid)
                    } else {
                        value as i64
                    };
                    patch[dimension] = serde_json::json!(value);
                }
            }
            if ["worldLayout", "worldGridWidth", "worldGridHeight"]
                .iter()
                .any(|key| patch.get(key).is_some())
            {
                p.set_world_layout(world, &layout, grid_w, grid_h)?;
                for key in ["worldLayout", "worldGridWidth", "worldGridHeight"] {
                    patch.as_object_mut().unwrap().remove(key);
                }
            }
            let target = match world {
                Some(i) => &mut p.json["worlds"][i],
                None => &mut p.json,
            };
            ui_fields::apply_patch(target, &patch);
            p.refresh_derived_data()
        };
        if layout_changed {
            self.edit_recoverable("World layout changed", operation);
        } else {
            self.edit("World settings changed", operation);
        }
        self.fit = true;
    }
    pub(super) fn create_form_world(&mut self) {
        if self.project.as_ref().is_none_or(Project::is_backup) {
            return;
        }
        if let Some(world) = self.edit("World created", |p| {
            let name = panels::unique_name(
                &p.cleanup_identifier("World"),
                p.json["worlds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|w| w["identifier"].as_str()),
            );
            let world = p.add_world(&name)?;
            let name = panels::unique_name(
                &p.cleanup_identifier("Level1"),
                p.level_keys()
                    .into_iter()
                    .filter_map(|k| p.level(k)?["identifier"].as_str()),
            );
            let level = p.add_level(Some(world), &name, 256, 256)?;
            p.level_mut(level).unwrap()["useAutoIdentifier"] = serde_json::json!(true);
            p.refresh_derived_data()?;
            Ok(world)
        }) {
            self.navigate_world(Some(world));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn editor() -> EditorApp {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.world = true;
        app.fit = false;
        app
    }
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        app.repaint = Some(ctx.clone());
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.properties(ui, Panel::World));
                app.world_form_dialog(ctx);
            },
        )
    }
    fn label(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(t)
                    if t.galley
                        .job
                        .text
                        .split_whitespace()
                        .eq(label.split_whitespace()) =>
                {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, label))
            .unwrap_or_else(|| panic!("missing {label}"))
    }
    fn click(app: &mut EditorApp, ctx: &egui::Context, name: &str) -> egui::FullOutput {
        frame(app, ctx, vec![]);
        let output = frame(app, ctx, vec![]);
        let pos = label(&output, name);
        frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
    }
    #[test]
    fn actual_form_duplicate_requires_confirmation_cancel_is_unchanged_and_copy_uses_source_offset()
    {
        let mut app = editor();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let before = app.project.as_ref().unwrap().json.clone();
        click(&mut app, &ctx, "Duplicate level");
        assert_eq!(
            app.project.as_ref().unwrap().json,
            before,
            "opening confirmation cannot duplicate"
        );
        click(&mut app, &ctx, "Cancel");
        assert_eq!(app.project.as_ref().unwrap().json, before);
        click(&mut app, &ctx, "Duplicate level");
        click(&mut app, &ctx, "Confirm");
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.level_keys().len(), 2);
        let copy = p.level(app.selected.unwrap()).unwrap();
        assert_eq!(
            (copy["worldX"].as_i64(), copy["worldY"].as_i64()),
            (Some(64), Some(64))
        );
        assert!(!copy["identifier"].as_str().unwrap().contains("_copy"));
        assert!(app.fit);
    }
    #[test]
    fn actual_form_last_level_guard_and_confirmed_delete_selects_closest_with_recovery() {
        let mut app = editor();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Delete level");
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(app.status.contains("last level"));
        let p = app.project.as_mut().unwrap();
        let far = p.add_level(None, "Far", 256, 256).unwrap();
        p.move_level(far, 2000, 0, 0).unwrap();
        let near = p.add_level(None, "Near", 256, 256).unwrap();
        p.move_level(near, 300, 0, 0).unwrap();
        let near_iid = p.level(near).unwrap()["iid"].clone();
        click(&mut app, &ctx, "Delete level");
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 3);
        click(&mut app, &ctx, "Confirm");
        assert_eq!(
            app.project
                .as_ref()
                .unwrap()
                .level(app.selected.unwrap())
                .unwrap()["iid"],
            near_iid
        );
        assert!(app.last_chance.is_some());
        assert!(app.world);
        app.restore_last_chance();
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 3);
    }
    #[test]
    fn actual_form_world_creation_uses_source_name_and_auto_level_identifier() {
        let mut app = editor();
        app.project.as_mut().unwrap().json["levelNamePattern"] = json!("%world_%idx");
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "+ World");
        let p = app.project.as_ref().unwrap();
        let key = app.selected.unwrap();
        assert_eq!(p.json["worlds"][key.world.unwrap()]["identifier"], "World");
        assert_eq!(p.level(key).unwrap()["useAutoIdentifier"], true);
        assert!(app.world);
        click(&mut app, &ctx, "+ World");
        let p = app.project.as_ref().unwrap();
        assert_eq!(
            p.json["worlds"][app.active_world.unwrap()]["identifier"],
            "World2"
        );
    }
    #[test]
    fn actual_form_world_delete_confirms_selects_first_other_and_guards_last_world() {
        let mut app = editor();
        let p = app.project.as_mut().unwrap();
        let second = p.add_world("Second").unwrap();
        p.add_level(Some(second), "SecondLevel", 256, 256).unwrap();
        let third = p.add_world("Third").unwrap();
        p.add_level(Some(third), "ThirdLevel", 256, 256).unwrap();
        app.navigate_world(Some(second));
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Delete world and its levels");
        assert_eq!(
            app.project.as_ref().unwrap().json["worlds"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        click(&mut app, &ctx, "Confirm");
        assert_eq!(app.active_world, Some(0));
        assert!(app.world);
        assert!(app.last_chance.is_some());
        click(&mut app, &ctx, "Delete world and its levels");
        click(&mut app, &ctx, "Confirm");
        click(&mut app, &ctx, "Delete world and its levels");
        assert_eq!(
            app.project.as_ref().unwrap().json["worlds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(app.status.contains("last world"));
    }
    #[test]
    fn actual_layout_choice_confirms_only_when_project_has_more_than_two_levels() {
        let mut app = editor();
        let p = app.project.as_mut().unwrap();
        p.add_level(None, "Second", 256, 256).unwrap();
        p.add_level(None, "Third", 256, 256).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Free");
        click(&mut app, &ctx, "LinearVertical");
        assert_eq!(app.project.as_ref().unwrap().json["worldLayout"], "Free");
        click(&mut app, &ctx, "CANCEL");
        assert_eq!(app.project.as_ref().unwrap().json["worldLayout"], "Free");
        click(&mut app, &ctx, "Free");
        click(&mut app, &ctx, "LinearVertical");
        click(&mut app, &ctx, "CONFIRM");
        assert_eq!(
            app.project.as_ref().unwrap().json["worldLayout"],
            "LinearVertical"
        );
        assert!(app.last_chance.is_some());
        let mut app = editor();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Free");
        click(&mut app, &ctx, "GridVania");
        assert_eq!(
            app.project.as_ref().unwrap().json["worldLayout"],
            "GridVania"
        );
        assert!(app.last_chance.is_some());
    }
    fn replace_text(app: &mut EditorApp, ctx: &egui::Context, value: &str) {
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            }],
        );
        frame(app, ctx, vec![egui::Event::Text(value.into())]);
        frame(
            app,
            ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn actual_world_identifier_edit_cleans_style_and_avoids_other_world_name() {
        let mut app = editor();
        let p = app.project.as_mut().unwrap();
        let target = p.add_world("Other").unwrap();
        p.add_level(Some(target), "OtherLevel", 256, 256).unwrap();
        let occupied = p.add_world("Taken").unwrap();
        p.add_level(Some(occupied), "TakenLevel", 256, 256).unwrap();
        app.navigate_world(Some(target));
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        // The selected world also appears in the selector; target the form input row.
        let output = frame(&mut app, &ctx, vec![]);
        let y = label(&output, "World identifier").y;
        let pos = egui::pos2(250., y);
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
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
        replace_text(&mut app, &ctx, " taken ");
        assert_eq!(
            app.project.as_ref().unwrap().json["worlds"][target]["identifier"],
            "Taken2"
        );
    }
    #[test]
    fn actual_default_size_edit_clamps_and_snaps_to_world_grid() {
        let mut app = editor();
        app.project.as_mut().unwrap().json["defaultLevelWidth"] = json!(151);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "151");
        replace_text(&mut app, &ctx, "1");
        assert_eq!(app.project.as_ref().unwrap().json["defaultLevelWidth"], 16);
        app.project
            .as_mut()
            .unwrap()
            .set_world_layout(None, "GridVania", 128, 64)
            .unwrap();
        app.project.as_mut().unwrap().json["defaultLevelWidth"] = json!(512);
        click(&mut app, &ctx, "512");
        replace_text(&mut app, &ctx, "193");
        assert_eq!(app.project.as_ref().unwrap().json["defaultLevelWidth"], 256);
    }
    #[test]
    fn actual_confirmation_keyboard_escape_cancels_and_enter_accepts() {
        let mut app = editor();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Duplicate level");
        assert!(is_open(&ctx));
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, &ctx, vec![key(egui::Key::Escape)]);
        assert!(!is_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 1);
        click(&mut app, &ctx, "Duplicate level");
        frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);
        assert!(!is_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);
    }
}
