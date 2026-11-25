use super::{panels::source_controls, *};
use std::collections::VecDeque;

pub(super) enum Continuation {
    None,
    Save(PathBuf, ldtk_core::FilesystemSnapshot),
    AfterSave(Option<String>),
}
pub(super) struct HookFlow {
    commands: VecDeque<String>,
    cwd: Option<PathBuf>,
    continuation: Continuation,
    waiting_trust: bool,
}

impl EditorApp {
    pub(super) fn queue_project_hooks(
        &mut self,
        ctx: &egui::Context,
        when: &str,
        continuation: Continuation,
    ) {
        let commands = self
            .project
            .as_ref()
            .map(|p| {
                p.json["customCommands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["when"] == when)
                    .filter_map(|c| c["command"].as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect::<VecDeque<_>>()
            })
            .unwrap_or_default();
        let cwd = self
            .project
            .as_ref()
            .and_then(|p| p.path.as_ref())
            .and_then(|p| p.parent())
            .map(std::path::Path::to_path_buf);
        self.queue_hook_commands(ctx, commands, cwd, continuation);
    }

    pub(super) fn queue_hook_commands(
        &mut self,
        ctx: &egui::Context,
        commands: VecDeque<String>,
        cwd: Option<PathBuf>,
        continuation: Continuation,
    ) {
        let trust = self
            .project
            .as_ref()
            .and_then(|p| self.settings.trust_state(text(&p.json, "iid", "")));
        if commands.is_empty() || trust == Some(false) {
            self.hook_continuation(ctx, continuation);
            return;
        }
        self.hook_flow = Some(HookFlow {
            commands,
            cwd,
            continuation,
            waiting_trust: trust.is_none(),
        });
        self.poll_hooks(ctx);
    }

    fn hook_continuation(&mut self, ctx: &egui::Context, continuation: Continuation) {
        match continuation {
            Continuation::None => {}
            Continuation::Save(path, snapshot) => self.save_async_at(path, Some(snapshot)),
            Continuation::AfterSave(saved_name) => {
                if let Some(name) = saved_name {
                    self.report_saved_project(name);
                }
                if !self.dirty
                    && let Some(action) = self.pending.take()
                {
                    self.clear_save_progress();
                    self.execute(ctx, action);
                }
            }
        }
    }

    pub(super) fn poll_hooks(&mut self, ctx: &egui::Context) {
        if self.command_result.is_some() || self.command_open {
            return;
        }
        let Some(mut flow) = self.hook_flow.take() else {
            return;
        };
        if flow.waiting_trust {
            self.hook_flow = Some(flow);
            return;
        }
        if let Some(command) = flow.commands.pop_front() {
            self.start_command(command, flow.cwd.clone());
            self.hook_flow = Some(flow);
        } else {
            self.hook_continuation(ctx, flow.continuation);
        }
    }

    pub(super) fn trust_dialog(&mut self, ctx: &egui::Context) {
        let Some(flow) = &self.hook_flow else {
            return;
        };
        if !flow.waiting_trust {
            return;
        }
        let decision = command_tools::show_dialog(ctx, "project-command-trust", |ui| {
            ui.label(
                RichText::new(locale::tr(ui, "This project contains custom commands")).strong(),
            );
            ui.label(locale::tr(
                ui,
                "Only allow commands from a project you trust.",
            ));
            ui.spacing_mut().scroll = source_controls::scroll_style();
            egui::ScrollArea::vertical()
                .max_height(180.)
                .show(ui, |ui| {
                    for command in &flow.commands {
                        ui.monospace(command);
                    }
                });
            ui.horizontal_wrapped(|ui| {
                let size = egui::vec2(170., 34.);
                let allow =
                    project_dialogs::button(ui, "Allow project commands", size, false, 13.333);
                let block =
                    project_dialogs::button(ui, "Block project commands", size, true, 13.333);
                let cancel = project_dialogs::button(ui, "Cancel", size, true, 13.333);
                if allow.clicked() {
                    Some(Some(true))
                } else if block.clicked() {
                    Some(Some(false))
                } else if cancel.clicked() {
                    Some(None)
                } else {
                    None
                }
            })
            .inner
        });
        if let Some(decision) = decision {
            if let Some(trust) = decision
                && let Some(project) = &self.project
            {
                self.settings
                    .set_trusted(text(&project.json, "iid", ""), trust);
            }
            if let Some(flow) = &mut self.hook_flow {
                flow.waiting_trust = false;
                if decision != Some(true) {
                    flow.commands.clear();
                }
            }
            self.poll_hooks(ctx);
        }
    }

    pub(super) fn save_async_at(
        &mut self,
        path: PathBuf,
        snapshot: Option<ldtk_core::FilesystemSnapshot>,
    ) {
        if !self.settle_deferred_auto_paint() {
            return;
        }
        self.finish_stroke();
        if self.reject_conflicting_save_folder(&path) {
            return;
        }
        let Some(mut project) = self.project.clone() else {
            return;
        };
        self.last_chance = None;
        let (tx, rx) = mpsc::channel();
        self.saving = Some(rx);
        self.save_attempt = Some(path.clone());
        let progress = Arc::new(Mutex::new(save_progress::SaveProgress::default()));
        self.saving_progress = Some(progress.clone());
        self.status = "Saving project…".into();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let names = project
                .level_keys()
                .into_iter()
                .filter_map(|key| {
                    project
                        .level(key)?
                        .get("identifier")?
                        .as_str()
                        .map(str::to_owned)
                })
                .collect::<Vec<_>>();
            let result = project
                .save_with_snapshot_and_progress(
                    &path,
                    snapshot.as_ref(),
                    |stage, done, total, label| {
                        progress
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .record(stage, done, total, label, &names);
                        if let Some(ctx) = &repaint {
                            ctx.request_repaint();
                        }
                    },
                )
                .map(|report| services::SavedProject { project, report });
            let _ = tx.send(result);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }
}
