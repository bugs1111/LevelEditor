use crate::{LevelKey, Project};
use serde_json::Value;

const MAX_ACTIONS: usize = 30;

struct Snapshot {
    key: LevelKey,
    level: Value,
}
impl Snapshot {
    fn capture(project: &Project, key: LevelKey) -> Option<Self> {
        Some(Self {
            key,
            level: project.level(key)?.clone(),
        })
    }
    fn restore(self, project: &mut Project) -> Option<Self> {
        let level = project.level_mut(self.key)?;
        Some(Self {
            key: self.key,
            level: std::mem::replace(level, self.level),
        })
    }
}

#[derive(Default)]
pub struct History {
    pending: Option<Snapshot>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl History {
    pub fn begin(&mut self, project: &Project, key: LevelKey) {
        if self.pending.is_none() {
            self.pending = Snapshot::capture(project, key);
        }
    }
    pub fn commit(&mut self, project: &Project) -> bool {
        let Some(snapshot) = self.pending.take() else {
            return false;
        };
        if project.level(snapshot.key) == Some(&snapshot.level) {
            return false;
        }
        self.undo.push(snapshot);
        if self.undo.len() > MAX_ACTIONS {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }
    pub fn undo(&mut self, project: &mut Project) -> bool {
        self.pending = None;
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        if let Some(inverse) = snapshot.restore(project) {
            self.redo.push(inverse);
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, project: &mut Project) -> bool {
        self.pending = None;
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        if let Some(inverse) = snapshot.restore(project) {
            self.undo.push(inverse);
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
