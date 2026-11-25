use super::*;
use std::io::Read;
use std::process::{Command, Stdio};

const EMPTY_COMMAND: &str = "Command is empty";

pub(super) enum CommandEvent {
    Output(String, bool),
    Done(Option<i32>),
    Error(String),
}

pub(super) fn process(command: &str, cwd: Option<&std::path::Path>) -> Result<Command, String> {
    let args = command_args::parse(command);
    let Some(name) = args.first().filter(|s| !s.is_empty()) else {
        return Err(EMPTY_COMMAND.into());
    };
    let mut child = Command::new(name);
    child.args(&args[1..]);
    if let Some(cwd) = cwd {
        child.current_dir(cwd);
    }
    Ok(child)
}

impl EditorApp {
    pub(super) fn run_manual_command(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let commands = project.json["customCommands"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["when"] == "Manual")
            .filter_map(|c| c["command"].as_str())
            .collect::<Vec<_>>();
        if commands.len() == 1 {
            let command = commands[0].to_owned();
            let cwd = project
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(std::path::Path::to_path_buf);
            self.run_command(command, cwd);
        } else {
            self.panel = Some(Panel::Export);
        }
    }
    pub(super) fn start_command(&mut self, command: String, cwd: Option<PathBuf>) {
        if self.command_result.is_some() {
            self.command_open = true;
            return;
        }
        if command.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(64);
        let (cancel_tx, cancel_rx) = mpsc::channel();
        self.command_result = Some(rx);
        self.command_cancel = Some(cancel_tx);
        self.command_output.clear();
        self.command_open = true;
        self.command_title = command.clone();
        self.command_cancelled = false;
        self.status = "Executing project command…".into();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Option<i32>, String> {
                let mut process = process(&command, cwd.as_deref())?;
                process
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdin(Stdio::null());
                let mut child = process.spawn().map_err(|e| e.to_string())?;
                let readers = [
                    (
                        child
                            .stdout
                            .take()
                            .map(|s| Box::new(s) as Box<dyn Read + Send>),
                        false,
                    ),
                    (
                        child
                            .stderr
                            .take()
                            .map(|s| Box::new(s) as Box<dyn Read + Send>),
                        true,
                    ),
                ]
                .into_iter()
                .filter_map(|(stream, error)| {
                    stream.map(|mut stream| {
                        let tx = tx.clone();
                        let repaint = repaint.clone();
                        std::thread::spawn(move || {
                            let mut bytes = [0u8; 4096];
                            loop {
                                match stream.read(&mut bytes) {
                                    Ok(0) | Err(_) => break,
                                    Ok(count) => {
                                        if tx
                                            .send(CommandEvent::Output(
                                                String::from_utf8_lossy(&bytes[..count])
                                                    .into_owned(),
                                                error,
                                            ))
                                            .is_err()
                                        {
                                            break;
                                        }
                                        if let Some(ctx) = &repaint {
                                            ctx.request_repaint();
                                        }
                                    }
                                }
                            }
                        })
                    })
                })
                .collect::<Vec<_>>();
                let exit = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break Ok(status.code()),
                        Ok(None) => {}
                        Err(e) => {
                            let _ = child.kill();
                            break Err(e.to_string());
                        }
                    }
                    if cancel_rx
                        .recv_timeout(std::time::Duration::from_millis(20))
                        .is_ok()
                    {
                        let _ = child.kill();
                        break child.wait().map(|s| s.code()).map_err(|e| e.to_string());
                    }
                };
                for reader in readers {
                    let _ = reader.join();
                }
                exit
            })();
            let event = match result {
                Ok(code) => CommandEvent::Done(code),
                Err(e) => CommandEvent::Error(e),
            };
            let _ = tx.send(event);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn poll_command(&mut self, ctx: &egui::Context) {
        let events = self
            .command_result
            .as_ref()
            .map(|rx| rx.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                CommandEvent::Output(text, error) => {
                    self.command_output.push_back((text, error));
                    while self.command_output.len() > 128 {
                        self.command_output.pop_front();
                    }
                }
                CommandEvent::Done(code) => {
                    self.command_result = None;
                    self.command_cancel = None;
                    self.status = match code {
                        Some(code) => locale::format_ctx(
                            ctx,
                            "Command terminated with {code}",
                            &[("code", &code.to_string())],
                        ),
                        None => locale::tr_ctx(ctx, "Command terminated with signal").into(),
                    };
                    self.command_output.push_back((self.status.clone(), false));
                    if code == Some(0) && !self.command_cancelled {
                        self.command_open = false;
                    }
                }
                CommandEvent::Error(error) => {
                    self.command_result = None;
                    self.command_cancel = None;
                    let detail = if error == EMPTY_COMMAND {
                        locale::tr_ctx(ctx, EMPTY_COMMAND)
                    } else {
                        &error
                    };
                    self.status =
                        locale::format_ctx(ctx, "Command failed ({error})", &[("error", detail)]);
                    self.status_error = true;
                    self.command_output.push_back((self.status.clone(), true));
                }
            }
        }
        if self.command_result.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    pub(super) fn command_dialog(&mut self, ctx: &egui::Context) {
        if !self.command_open {
            return;
        }
        egui::Modal::new(egui::Id::new("command-runner")).show(ctx, |ui| {
            ui.set_width(660.0);
            ui.heading(locale::tr(ui, "Project command"));
            ui.monospace(&self.command_title);
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(420.0)
                .show(ui, |ui| {
                    ui.set_min_height(160.0);
                    for (text, error) in &self.command_output {
                        ui.label(RichText::new(text).monospace().color(if *error {
                            theme::ACCENT
                        } else {
                            Color32::WHITE
                        }));
                    }
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.command_result.is_some(),
                        egui::Button::new(locale::tr(ui, "Kill process")),
                    )
                    .clicked()
                {
                    self.command_cancelled = true;
                    if let Some(cancel) = &self.command_cancel {
                        let _ = cancel.send(());
                    }
                }
                if ui
                    .add_enabled(
                        self.command_result.is_none(),
                        egui::Button::new(locale::tr(ui, "Close")),
                    )
                    .clicked()
                {
                    self.command_open = false;
                }
            });
        });
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn command_output_streams_before_completion_and_cancel_terminates_it() {
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        app.start_command("python3 -c \"import os,time; os.write(1, bytes([114,101,97,100,121])); time.sleep(30)\"".into(),None);
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        while app.command_output.is_empty() && Instant::now() < deadline {
            app.poll_command(&ctx);
            std::thread::yield_now();
        }
        assert!(app.command_output.iter().any(|(text, _)| text == "ready"));
        assert!(app.command_result.is_some());
        app.command_cancel.as_ref().unwrap().send(()).unwrap();
        while app.command_result.is_some() && Instant::now() < deadline {
            app.poll_command(&ctx);
            std::thread::yield_now();
        }
        assert!(
            app.command_result.is_none(),
            "Cancel must terminate and reap the child process"
        );
    }
}
