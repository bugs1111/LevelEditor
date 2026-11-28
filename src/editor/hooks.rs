use super::*;
use std::collections::VecDeque;

pub(super) enum Continuation {
    None,
    Save(PathBuf, ldtk_core::FilesystemSnapshot),
    AfterSave(Option<String>),
}
pub(super) struct HookFlow {
    project_iid: String,
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
        let Some(project) = self.project.as_ref().filter(|_| !commands.is_empty()) else {
            self.hook_continuation(ctx, continuation);
            return;
        };
        self.hook_flow = Some(HookFlow {
            project_iid: text(&project.json, "iid", "").to_owned(),
            commands,
            cwd,
            continuation,
            waiting_trust: false,
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
        if flow.commands.is_empty() || !self.refresh_command_trust() {
            self.hook_continuation(ctx, flow.continuation);
            return;
        }
        match self.settings.trust_state(&flow.project_iid) {
            Some(false) => self.hook_continuation(ctx, flow.continuation),
            Some(true) => {
                self.start_command(flow.commands.pop_front().unwrap(), flow.cwd.clone());
                self.hook_flow = Some(flow);
            }
            None => {
                flow.waiting_trust = true;
                self.hook_flow = Some(flow);
            }
        }
    }

    pub(super) fn trust_dialog(&mut self, ctx: &egui::Context) {
        let Some(flow) = &self.hook_flow else {
            return;
        };
        if !flow.waiting_trust {
            return;
        }
        let decision = project_dialogs::show_dialog(
            ctx,
            egui::Id::new("project-command-trust"),
            "Project command",
            620.,
            |ui| {
                ui.label(
                    RichText::new(locale::tr(ui, "This project contains custom commands"))
                        .font(theme::TextRole::StrongBody.font()),
                );
                ui.label(locale::tr(
                    ui,
                    "Only allow commands from a project you trust.",
                ));
                egui::ScrollArea::vertical()
                    .max_height(180.)
                    .show(ui, |ui| {
                        for command in &flow.commands {
                            ui.monospace(command);
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    let allow = project_dialogs::action_with_width(
                        ui,
                        "Allow project commands",
                        true,
                        170.,
                    );
                    let block = project_dialogs::action_with_width(
                        ui,
                        "Block project commands",
                        false,
                        170.,
                    );
                    let cancel = project_dialogs::action_with_width(ui, "Cancel", false, 170.);
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
            },
        )
        .inner;
        if let Some(decision) = decision {
            if let Some(trusted) = decision {
                let iid = flow.project_iid.clone();
                let saved = self.change_command_trust(&iid, Some(trusted));
                if trusted && !saved {
                    return;
                }
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

    pub(super) fn refresh_command_trust(&mut self) -> bool {
        let result = self.settings.refresh_trust();
        self.command_trust_result(result)
    }

    pub(super) fn change_command_trust(&mut self, iid: &str, trusted: Option<bool>) -> bool {
        let result = self.settings.set_trust(iid, trusted);
        if !self.command_trust_result(result) {
            return false;
        }
        self.set_operation_status("Project command permissions saved.");
        true
    }

    fn command_trust_result(&mut self, result: std::io::Result<()>) -> bool {
        if let Err(error) = result {
            let label = self
                .settings
                .language()
                .translate("Project command permissions");
            self.status = format!("{label}: {error}");
            self.status_error = true;
            return false;
        }
        true
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
        self.set_operation_status("Saving project…");
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
                .map(|report| services::SavedProject {
                    project,
                    report,
                    renamed_from: None,
                });
            let _ = tx.send(result);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queued_hook_observes_a_block_applied_from_another_window() {
        let directory = tempfile::tempdir().unwrap();
        let path = Some(directory.path().join("settings.json"));
        let mut other = settings::Settings::load_at(path.clone());
        let mut app = EditorApp::empty();
        let project = Project::new();
        let iid = text(&project.json, "iid", "").to_owned();
        other.set_trust(&iid, Some(true)).unwrap();
        app.settings = settings::Settings::load_at(path);
        app.project = Some(project);
        app.command_open = true;
        let ctx = egui::Context::default();
        app.queue_hook_commands(
            &ctx,
            VecDeque::from(["echo sample".to_owned()]),
            None,
            Continuation::None,
        );
        assert!(app.hook_flow.is_some());
        other.set_trust(&iid, Some(false)).unwrap();
        app.command_open = false;
        app.poll_hooks(&ctx);
        assert!(
            app.hook_flow.is_none(),
            "a blocked queue must finish before dispatch"
        );
        assert_eq!(app.settings.trust_state(&iid), Some(false));
        assert!(app.command_result.is_none() && !app.command_open);
    }
}
