use journal_domain::RecordSource;
use journal_storage::Database;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub revision: i64,
    pub source: RecordSource,
    pub entity: String,
}

/// Coordinates both in-process real-time updates (embedded HTTP MCP / GUI actions)
/// and cross-process change detection (external STDIO MCP processes writing to SQLite).
pub struct SyncCoordinator {
    db: Arc<Database>,
    last_revision: AtomicI64,
    tx: broadcast::Sender<SyncEvent>,
}

impl SyncCoordinator {
    pub fn new(db: Arc<Database>) -> Self {
        let initial_rev = db.get_sync_state().map(|s| s.revision).unwrap_or(0);
        let (tx, _) = broadcast::channel(128);
        Self {
            db,
            last_revision: AtomicI64::new(initial_rev),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.tx.subscribe()
    }

    pub fn current_revision(&self) -> i64 {
        self.db
            .get_sync_state()
            .map(|s| s.revision)
            .unwrap_or_else(|_| self.last_revision.load(Ordering::SeqCst))
    }

    pub fn notify_local_change(&self, source: RecordSource, entity: impl Into<String>) {
        if let Ok(state) = self.db.get_sync_state() {
            self.last_revision.store(state.revision, Ordering::SeqCst);
            let _ = self.tx.send(SyncEvent {
                revision: state.revision,
                source,
                entity: entity.into(),
            });
        }
    }

    /// Checks whether another process (e.g., standalone `student-journal-mcp`) mutated SQLite.
    /// Returns `Some(SyncEvent)` if the database revision advanced since last observation.
    pub fn poll_external_changes(&self) -> Option<SyncEvent> {
        let state = self.db.get_sync_state().ok()?;
        let prev = self.last_revision.load(Ordering::SeqCst);
        if state.revision > prev {
            self.last_revision.store(state.revision, Ordering::SeqCst);
            let event = SyncEvent {
                revision: state.revision,
                source: state.last_source,
                entity: "external_db_change".into(),
            };
            let _ = self.tx.send(event.clone());
            Some(event)
        } else {
            None
        }
    }
}
