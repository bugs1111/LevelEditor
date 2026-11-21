//! Commit focused form drafts before actions which replace, save, or leave them.
use super::*;

pub(super) enum DeferredUiAction {
    Save(bool),
    Request(Action),
    Panel(Option<Panel>),
    Navigate {
        key: LevelKey,
        fit: bool,
    },
    NeighborLevel(LevelKey),
    ScrollToLevel {
        key: LevelKey,
        previous_origin: [f32; 2],
    },
    World {
        world: Option<usize>,
        open_world_view: bool,
    },
    FollowEntity(String),
    Search(search::Destination),
    Zoom {
        rect: Rect,
        pointer: egui::Pos2,
        deltas: Vec<f32>,
    },
}

#[derive(Default)]
pub(super) struct UiCommit {
    action: Option<(u64, DeferredUiAction)>,
    pointer_events: std::collections::VecDeque<(Vec<egui::Event>, egui::Modifiers)>,
    flushing: bool,
}
impl UiCommit {
    pub(super) fn pending(&self) -> bool {
        self.action.is_some()
    }
}

#[derive(Clone)]
struct FocusedEditor {
    id: egui::Id,
    rect: egui::Rect,
}
fn editor_key() -> egui::Id {
    egui::Id::new("focused-form-editor")
}
fn blur_key() -> egui::Id {
    egui::Id::new("commit-form-editor")
}
fn focused_editor(ctx: &egui::Context) -> Option<FocusedEditor> {
    let editor = ctx.data_mut(|d| d.get_temp::<FocusedEditor>(editor_key()))?;
    ctx.memory(|m| m.has_focus(editor.id)).then_some(editor)
}

pub(super) fn has_pending_ui_edit(ctx: &egui::Context) -> bool {
    panels::source_colors::picker_is_open(ctx)
        || text_editor::is_open(ctx)
        || focused_editor(ctx).is_some()
        || ctx
            .data_mut(|d| d.get_temp::<(egui::Id, u64)>(blur_key()))
            .is_some()
}

/// Run immediately before the original widget, after egui has begun the frame.
/// Surrendering in raw_input_hook would erase egui's previous-focus bookkeeping.
pub(super) fn prepare_editor(ctx: &egui::Context, id: egui::Id) {
    if ctx
        .data_mut(|d| d.get_temp::<(egui::Id, u64)>(blur_key()))
        .is_some_and(|(target, frame)| target == id && ctx.cumulative_frame_nr() >= frame)
    {
        ctx.memory_mut(|m| m.surrender_focus(id));
        ctx.data_mut(|d| d.remove::<(egui::Id, u64)>(blur_key()));
    }
}
pub(super) fn track_editor(ctx: &egui::Context, response: &egui::Response) {
    if response.has_focus() {
        ctx.data_mut(|d| {
            d.insert_temp(
                editor_key(),
                FocusedEditor {
                    id: response.id,
                    rect: response.rect,
                },
            )
        });
    } else if ctx
        .data_mut(|d| d.get_temp::<FocusedEditor>(editor_key()))
        .is_some_and(|e| e.id == response.id)
    {
        ctx.data_mut(|d| d.remove::<FocusedEditor>(editor_key()));
    }
}

impl EditorApp {
    /// Return true when the caller must leave its current panel/document intact.
    pub(super) fn defer_ui_action(
        &mut self,
        ctx: &egui::Context,
        action: DeferredUiAction,
    ) -> bool {
        if self.ui_commit.flushing {
            return false;
        }
        if self.ui_commit.action.is_some() {
            return true;
        }
        let id = focused_editor(ctx).map(|e| e.id).or_else(|| {
            ctx.data_mut(|d| d.get_temp::<(egui::Id, u64)>(blur_key()))
                .map(|(id, _)| id)
        });
        let color_open = panels::source_colors::picker_is_open(ctx);
        let text_open = text_editor::is_open(ctx);
        if id.is_none() && !color_open && !text_open {
            return false;
        }
        if color_open {
            panels::source_colors::request_validate(ctx);
        }
        if text_open {
            text_editor::request_validate(ctx);
        }
        // Let the original editor consume any preceding text events from this
        // batch before blurring next frame. Remove the handled app shortcut so
        // e.g. Ctrl+W cannot also delete a word in the still-focused TextEdit.
        ctx.input_mut(|input| input.events.retain(|event| !matches!(event,
            egui::Event::Key { key: Key::S | Key::W | Key::Q | Key::O | Key::N, modifiers, .. } if modifiers.command
        )));
        let frame = ctx.cumulative_frame_nr();
        if let Some(id) = id {
            ctx.data_mut(|d| d.insert_temp(blur_key(), (id, frame + 1)));
        }
        self.ui_commit.action = Some((frame, action));
        ctx.request_repaint();
        true
    }

    /// Call after every old panel/form has had a chance to commit its draft.
    pub(super) fn flush_ui_action(&mut self, ctx: &egui::Context) {
        let Some((frame, _)) = self.ui_commit.action.as_ref() else {
            return;
        };
        if ctx.cumulative_frame_nr() <= *frame
            || panels::source_colors::picker_is_open(ctx)
            || text_editor::is_open(ctx)
            || ctx
                .data_mut(|d| d.get_temp::<(egui::Id, u64)>(blur_key()))
                .is_some()
        {
            ctx.request_repaint();
            return;
        }
        let (_, action) = self.ui_commit.action.take().unwrap();
        self.ui_commit.flushing = true;
        match action {
            DeferredUiAction::Save(save_as) => {
                self.save(save_as);
            }
            DeferredUiAction::Request(action) => self.request(ctx, action),
            DeferredUiAction::Panel(panel) => self.show_panel(panel),
            DeferredUiAction::Navigate { key, fit } => self.navigate_level(key, fit),
            DeferredUiAction::NeighborLevel(key) => self.navigate_neighbor_level(ctx, key),
            DeferredUiAction::ScrollToLevel { key, previous_origin } => {
                self.scroll_to_context_level(key, previous_origin);
            }
            DeferredUiAction::World {
                world,
                open_world_view,
            } => self.switch_world(world, open_world_view),
            DeferredUiAction::FollowEntity(iid) => {
                self.follow_entity(&iid);
            }
            DeferredUiAction::Search(destination) => self.select_search_destination(destination),
            DeferredUiAction::Zoom {
                rect,
                pointer,
                deltas,
            } => self.zoom_from_input(ctx, rect, pointer, deltas),
        }
        self.ui_commit.flushing = false;
    }

    /// Match the browser's blur-before-click ordering even when the sidebar
    /// renders before the form. Replay the complete event suffix next frame so
    /// fast press/release pairs and subsequent text keep their original order.
    pub(super) fn ui_commit_raw_input(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        focus::normalize_platform_pointer(ctx, input);
        if text_editor::is_open(ctx) {
            return;
        }
        if !self.ui_commit.pointer_events.is_empty() {
            // A replay frame keeps its original modifier state. Newly arrived
            // input stays in a later batch, rather than inheriting stale Ctrl/Cmd.
            if !input.events.is_empty() {
                self.ui_commit
                    .pointer_events
                    .push_back((std::mem::take(&mut input.events), input.modifiers));
            }
            if panels::source_colors::picker_is_open(ctx)
                || ctx
                    .data_mut(|d| d.get_temp::<(egui::Id, u64)>(blur_key()))
                    .is_some()
            {
                ctx.request_repaint();
                return;
            }
            let (events, modifiers) = self.ui_commit.pointer_events.pop_front().unwrap();
            input.events = events;
            input.modifiers = modifiers;
            if !self.ui_commit.pointer_events.is_empty() {
                ctx.request_repaint();
            }
        }
        let editor = focused_editor(ctx);
        let color = panels::source_colors::picker_rect(ctx)
            .filter(|_| panels::source_colors::picker_is_open(ctx));
        let rect = color.or_else(|| editor.as_ref().map(|editor| editor.rect));
        let Some(rect) = rect else {
            return;
        };
        let Some(index) = input.events.iter().position(|event| {
            matches!(event,
                egui::Event::PointerButton { pos, pressed: true, .. } if !rect.contains(*pos)
            )
        }) else {
            return;
        };
        self.ui_commit
            .pointer_events
            .push_front((input.events.split_off(index), input.modifiers));
        let has_text_input = input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Cut
                    | egui::Event::Key { .. }
                    | egui::Event::Ime(_)
            )
        });
        let frame = ctx.cumulative_frame_nr() + u64::from(has_text_input);
        if color.is_some() {
            panels::source_colors::request_validate(ctx);
        } else if let Some(editor) = editor {
            ctx.data_mut(|d| d.insert_temp(blur_key(), (editor.id, frame)));
        }
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEVEL: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    struct Form {
        app: EditorApp,
        ctx: egui::Context,
        uid: i64,
        rect: egui::Rect,
        switch: egui::Rect,
    }
    impl Form {
        fn new(kind: &str) -> Self {
            let mut p = Project::new();
            let uid = p.add_field_definition(None, "Draft", kind, false).unwrap();
            p.update_field_definition(uid, &serde_json::json!({"canBeNull":false}))
                .unwrap();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            app.set_project(p);
            app.panel = Some(Panel::Level);
            Self {
                app,
                ctx,
                uid,
                rect: egui::Rect::NOTHING,
                switch: egui::Rect::NOTHING,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) {
            let modifiers = events
                .iter()
                .find_map(|e| match e {
                    egui::Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default();
            let mut input = egui::RawInput {
                modifiers,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640., 480.),
                )),
                events,
                ..Default::default()
            };
            self.app.ui_commit_raw_input(&self.ctx, &mut input);
            let _ = self.ctx.run(input, |ctx| {
                self.app.shortcuts(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    let switch = ui.button("Switch panel");
                    self.switch = switch.rect;
                    // Model a sidebar selection rendered before the old form. The raw
                    // hook must guarantee blur even for selectors without action guards.
                    if switch.clicked() {
                        self.app.panel = Some(Panel::Help);
                    }
                    if self.app.panel == Some(Panel::Level) && self.app.project.is_some() {
                        let p = self.app.project.as_ref().unwrap();
                        let def = p.json["defs"]["levelFields"][0].clone();
                        let field = p.level(LEVEL).unwrap()["fieldInstances"][0].clone();
                        let r = ui.scope(|ui| ui_fields::field_value_editor(ui, p, &field, &def));
                        self.rect = r.response.rect;
                        if let Some(v) = r.inner {
                            self.app
                                .edit("Field changed", |p| v.apply(p, LEVEL, None, self.uid));
                        }
                    }
                });
                self.app.flush_ui_action(ctx);
            });
        }
        fn focus_and_type(&mut self, text: &str) {
            self.frame(vec![]);
            let pos = self.rect.min + egui::vec2(10., 10.);
            self.frame(vec![egui::Event::PointerMoved(pos), click(pos, true)]);
            self.frame(vec![click(pos, false)]);
            self.frame(vec![]);
            self.frame(vec![egui::Event::Text(text.into())]);
            assert!(!self.app.dirty, "only the input draft has changed");
        }
        fn command(&mut self, key: Key) {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    mac_cmd: true,
                    ..Default::default()
                },
            }]);
        }
        fn value(&self) -> Value {
            self.app.project.as_ref().unwrap().level(LEVEL).unwrap()["fieldInstances"][0]["__value"]
                .clone()
        }
    }
    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn multiline_modal_save_commits_then_publishes_exact_text_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Text.ldtk");
        let mut f = Form::new("Multilines");
        f.app.project.as_mut().unwrap().save(&path).unwrap();
        f.focus_and_type("hello");
        assert!(text_editor::is_open(&f.ctx));
        f.frame(vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        f.frame(vec![egui::Event::Text("world".into())]);
        assert!(!f.app.dirty);
        f.command(Key::S);
        f.frame(vec![]);
        f.frame(vec![]);
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while f.app.saving.is_some() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
            f.app.poll_services(&f.ctx);
        }
        assert!(!text_editor::is_open(&f.ctx));
        assert_eq!(
            Project::load(&path).unwrap().level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            "hello\\nworld"
        );
        assert!(f.app.history.undo(f.app.project.as_mut().unwrap()));
        assert!(!f.app.history.undo(f.app.project.as_mut().unwrap()));
    }
    #[test]
    fn multiline_modal_pairs_keep_caret_and_escape_commits() {
        let mut f = Form::new("Multilines");
        f.focus_and_type("(");
        f.frame(vec![egui::Event::Text("inside".into())]);
        f.frame(vec![egui::Event::Text(")".into())]);
        f.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(f.value(), "(inside)");
        assert!(!text_editor::is_open(&f.ctx));
    }
    #[test]
    fn batched_typed_delimiters_follow_event_order_and_paste_stays_literal() {
        for (events, expected) in [
            (
                vec![
                    egui::Event::Text("(".into()),
                    egui::Event::Text("word".into()),
                ],
                "(word)",
            ),
            (
                vec![
                    egui::Event::Paste("(".into()),
                    egui::Event::Text("word".into()),
                ],
                "(word",
            ),
        ] {
            let mut f = Form::new("Multilines");
            f.focus_and_type("");
            f.frame(events);
            f.frame(vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }]);
            assert_eq!(f.value(), expected);
        }
        let mut f = Form::new("Multilines");
        f.focus_and_type("(");
        f.frame(vec![egui::Event::Text("a".into())]);
        f.frame(vec![
            egui::Event::Text(")".into()),
            egui::Event::Text("b".into()),
        ]);
        f.frame(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(f.value(), "(a)b");
    }
    #[test]
    fn command_save_validates_color_hex_draft_before_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Color.ldtk");
        let mut f = Form::new("Color");
        f.app.project.as_mut().unwrap().save(&path).unwrap();
        f.focus_and_type("123");
        assert!(panels::source_colors::picker_is_open(&f.ctx));
        f.command(Key::S);
        f.frame(vec![]);
        f.frame(vec![]);
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while f.app.saving.is_some() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
            f.app.poll_services(&f.ctx);
        }
        assert_eq!(
            Project::load(&path).unwrap().level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            "#112233"
        );
    }
    #[test]
    fn outside_panel_click_validates_color_before_owner_disappears() {
        let mut f = Form::new("Color");
        f.focus_and_type("0af");
        let pos = f.switch.center();
        f.frame(vec![
            egui::Event::PointerMoved(pos),
            click(pos, true),
            click(pos, false),
        ]);
        f.frame(vec![]);
        f.frame(vec![]);
        f.frame(vec![]);
        assert_eq!(f.value(), "#00AAFF");
        assert!(f.app.panel == Some(Panel::Help));
        assert!(f.app.history.undo(f.app.project.as_mut().unwrap()));
        assert!(
            !f.app.history.undo(f.app.project.as_mut().unwrap()),
            "picker commits one transaction"
        );
    }
    #[test]
    fn command_save_publishes_focused_string_and_numeric_drafts() {
        for (kind, text, expected) in [
            ("String", "draft words", serde_json::json!("draft words")),
            ("Int", "42", serde_json::json!(42)),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("Draft.ldtk");
            let mut f = Form::new(kind);
            f.app.project.as_mut().unwrap().save(&path).unwrap();
            f.focus_and_type(text);
            f.command(Key::S);
            f.frame(vec![]);
            let until = Instant::now() + std::time::Duration::from_secs(5);
            while f.app.saving.is_some() && Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(5));
                f.app.poll_services(&f.ctx);
            }
            let saved = Project::load(&path).unwrap();
            assert_eq!(
                saved.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
                expected
            );
        }
    }
    #[test]
    fn command_close_prompts_for_a_draft_even_if_document_was_clean() {
        let mut f = Form::new("String");
        f.focus_and_type("unsaved");
        f.command(Key::W);
        f.frame(vec![]);
        assert!(
            f.app.project.is_some(),
            "a focused draft must prevent clean-close"
        );
        assert!(matches!(f.app.pending, Some(Action::Close)));
        assert!(f.app.dirty);
        assert_eq!(f.value(), "unsaved");
    }
    #[test]
    fn final_keystroke_batched_with_close_is_not_lost() {
        let mut f = Form::new("String");
        f.focus_and_type("first");
        f.frame(vec![
            egui::Event::Text(" last".into()),
            egui::Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            },
        ]);
        f.frame(vec![]);
        assert!(matches!(f.app.pending, Some(Action::Close)));
        assert_eq!(f.value(), "first last");
    }
    #[test]
    fn final_keystroke_batched_with_pointer_navigation_is_not_lost() {
        let mut f = Form::new("String");
        f.focus_and_type("first");
        let pos = f.switch.center();
        f.frame(vec![
            egui::Event::Text(" last".into()),
            egui::Event::PointerMoved(pos),
            click(pos, true),
            click(pos, false),
        ]);
        f.frame(vec![]);
        f.frame(vec![]);
        assert!(f.app.panel == Some(Panel::Help));
        assert_eq!(f.value(), "first last");
    }
    #[test]
    fn delayed_pointer_batch_preserves_modifiers_and_later_key_order() {
        let mut f = Form::new("String");
        f.focus_and_type("draft");
        let pos = f.switch.center();
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let mut press = egui::RawInput {
            modifiers,
            events: vec![egui::Event::PointerButton {
                pos,
                pressed: true,
                button: egui::PointerButton::Primary,
                modifiers,
            }],
            ..Default::default()
        };
        f.app.ui_commit_raw_input(&f.ctx, &mut press);
        assert!(press.events.is_empty());
        // Draw the old form to complete its blur, without invoking raw_input_hook twice.
        let _ = f.ctx.run(press, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Preserve the fixture's widget IDs and positions.
                ui.button("Switch panel").clicked();
                let p = f.app.project.as_ref().unwrap();
                ui.scope(|ui| {
                    ui_fields::field_value_editor(
                        ui,
                        p,
                        &p.level(LEVEL).unwrap()["fieldInstances"][0],
                        &p.json["defs"]["levelFields"][0],
                    )
                });
            });
        });
        let mut later = egui::RawInput {
            events: vec![egui::Event::Text("later".into())],
            ..Default::default()
        };
        f.app.ui_commit_raw_input(&f.ctx, &mut later);
        assert!(later.modifiers.ctrl);
        assert!(matches!(
            later.events.as_slice(),
            [egui::Event::PointerButton { .. }]
        ));
        let mut next = egui::RawInput::default();
        f.app.ui_commit_raw_input(&f.ctx, &mut next);
        assert!(!next.modifiers.ctrl);
        assert!(matches!(next.events.as_slice(), [egui::Event::Text(text)] if text == "later"));
    }
    #[test]
    fn pointer_panel_switch_blurs_before_old_editor_is_removed() {
        let mut f = Form::new("String");
        f.focus_and_type("keep this");
        let pos = f.switch.center();
        f.frame(vec![
            egui::Event::PointerMoved(pos),
            click(pos, true),
            click(pos, false),
        ]);
        assert!(
            f.app.panel == Some(Panel::Level),
            "switch is delayed for the old form to commit"
        );
        f.frame(vec![]);
        assert!(f.app.panel == Some(Panel::Help));
        assert!(f.app.dirty);
        assert_eq!(f.value(), "keep this");
    }
}
