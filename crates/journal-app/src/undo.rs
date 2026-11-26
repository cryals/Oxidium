use journal_domain::{AttendanceEntry, GradeEntry, Task};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use uuid::Uuid;

/// Reversible command snapshots stored in the Undo/Redo stack and optionally inside `activity_log`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UndoCommand {
    GradeAdded {
        grade: GradeEntry,
    },
    GradeUpdated {
        before: GradeEntry,
        after: GradeEntry,
    },
    GradeDeleted {
        grade: GradeEntry,
    },
    AttendanceAdded {
        entry: AttendanceEntry,
    },
    AttendanceDeleted {
        entry: AttendanceEntry,
    },
    TaskCreated {
        task: Task,
    },
    TaskUpdated {
        before: Task,
        after: Task,
    },
    TaskDeleted {
        task: Task,
    },
    BatchGradesAdded {
        grade_ids: Vec<Uuid>,
        grades: Vec<GradeEntry>,
    },
}

pub struct UndoStack {
    undo: Mutex<Vec<UndoCommand>>,
    redo: Mutex<Vec<UndoCommand>>,
    max_depth: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(100)
    }
}

impl UndoStack {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo: Mutex::new(Vec::new()),
            redo: Mutex::new(Vec::new()),
            max_depth,
        }
    }

    pub fn push(&self, cmd: UndoCommand) {
        if let Ok(mut u) = self.undo.lock() {
            u.push(cmd);
            if u.len() > self.max_depth {
                u.remove(0);
            }
        }
        if let Ok(mut r) = self.redo.lock() {
            r.clear();
        }
    }

    pub fn pop_undo(&self) -> Option<UndoCommand> {
        self.undo.lock().ok()?.pop()
    }

    pub fn push_redo(&self, cmd: UndoCommand) {
        if let Ok(mut r) = self.redo.lock() {
            r.push(cmd);
        }
    }

    pub fn pop_redo(&self) -> Option<UndoCommand> {
        self.redo.lock().ok()?.pop()
    }

    pub fn push_undo_without_clearing_redo(&self, cmd: UndoCommand) {
        if let Ok(mut u) = self.undo.lock() {
            u.push(cmd);
        }
    }

    pub fn can_undo(&self) -> bool {
        self.undo.lock().map(|u| !u.is_empty()).unwrap_or(false)
    }

    pub fn can_redo(&self) -> bool {
        self.redo.lock().map(|r| !r.is_empty()).unwrap_or(false)
    }
}
