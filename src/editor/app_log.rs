use super::*;
use std::{
    io::Write,
    path::Path,
    sync::{OnceLock, mpsc::SyncSender},
};

pub(super) fn path(settings: Option<&Path>) -> Option<PathBuf> {
    Some(settings?.parent()?.join("LDtk.log"))
}
fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}
enum Message {
    Entry(PathBuf, String),
    Flush(std::sync::mpsc::Sender<()>),
}
fn writer() -> &'static SyncSender<Message> {
    static WRITER: OnceLock<SyncSender<Message>> = OnceLock::new();
    WRITER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Message>(128);
        std::thread::spawn(move || {
            while let Ok(message) = rx.recv() {
                match message {
                    Message::Entry(path, line) => {
                        let _ = append(&path, &line);
                    }
                    Message::Flush(reply) => {
                        let _ = reply.send(());
                    }
                }
            }
        });
        tx
    })
}
pub(super) fn record(ctx: &egui::Context, settings: Option<&Path>, message: &str, error: bool) {
    let Some(path) = path(settings) else { return };
    let key = egui::Id::new("last-app-log-status");
    let previous = ctx.data_mut(|d| d.get_temp::<(String, bool)>(key));
    if previous
        .as_ref()
        .is_some_and(|(last, level)| last == message && *level == error)
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, (message.to_owned(), error)));
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let line = format!(
        "[{timestamp}] {} {message}\n",
        if error { "ERROR" } else { "INFO" }
    );
    let _ = writer().try_send(Message::Entry(path, line));
}
pub(super) fn flush() {
    let (tx, rx) = std::sync::mpsc::channel();
    if writer().send(Message::Flush(tx)).is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(1));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_appends_without_replacing_previous_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let log = path(Some(&dir.path().join("settings.json"))).unwrap();
        append(&log, "first\n").unwrap();
        append(&log, "错误\n").unwrap();
        assert_eq!(std::fs::read_to_string(log).unwrap(), "first\n错误\n");
    }
}
