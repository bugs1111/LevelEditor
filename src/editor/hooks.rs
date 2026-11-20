use super::*;
use std::collections::VecDeque;

pub(super) enum Continuation {
    None,
    Save(PathBuf),
    AfterSave,
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
            Continuation::Save(path) => self.save_async_at(path),
            Continuation::AfterSave => {
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
        let commands = flow.commands.iter().cloned().collect::<Vec<_>>();
        let mut decision = None;
        egui::Modal::new(egui::Id::new("project-command-trust")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.heading("This project contains custom commands");
            ui.label("Only allow commands from a project you trust.");
            for command in commands {
                ui.monospace(command);
            }
            ui.horizontal(|ui| {
                if ui.button("Allow project commands").clicked() {
                    decision = Some(Some(true));
                }
                if ui.button("Block project commands").clicked() {
                    decision = Some(Some(false));
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(None);
                }
            });
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

    pub(super) fn save_async_at(&mut self, path: PathBuf) {
        if !self.settle_deferred_auto_paint() {
            return;
        }
        self.finish_stroke();
        let Some(mut project) = self.project.clone() else {
            return;
        };
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
                .save_with_labeled_operation_progress(&path, |stage, done, total, label| {
                    progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .record(stage, done, total, label, &names);
                    if let Some(ctx) = &repaint {
                        ctx.request_repaint();
                    }
                })
                .map(|report| services::SavedProject { project, report });
            let _ = tx.send(result);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }
}
