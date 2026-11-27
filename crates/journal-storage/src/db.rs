use chrono::{DateTime, Utc};
use journal_domain::{RecordSource, builtin_marks};
use rusqlite::{Connection, backup, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use crate::error::{StorageError, StorageResult};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

fn canonical_migration_sql(sql: &str) -> String {
    sql.replace("\r\n", "\n")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_initial_schema",
        sql: include_str!("../../../migrations/0001_initial_schema.sql"),
    },
    Migration {
        version: 2,
        name: "0002_fts5_and_indexes",
        sql: include_str!("../../../migrations/0002_fts5_and_indexes.sql"),
    },
    Migration {
        version: 3,
        name: "0003_sync_and_idempotency",
        sql: include_str!("../../../migrations/0003_sync_and_idempotency.sql"),
    },
];

#[derive(Debug, Clone)]
pub struct SyncState {
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
    pub last_source: RecordSource,
}

#[derive(Debug, Clone)]
pub struct AppliedMigration {
    pub version: i64,
    pub name: String,
    pub applied_at: String,
    pub checksum: String,
}

pub struct Database {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path_buf)?;
        Self::configure_connection(&conn, true)?;
        let db = Self {
            conn: Mutex::new(conn),
            path: Some(path_buf),
        };
        db.run_migrations()?;
        db.seed_builtin_mark_types()?;
        Ok(db)
    }

    pub fn open_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure_connection(&conn, false)?;
        let db = Self {
            conn: Mutex::new(conn),
            path: None,
        };
        db.run_migrations()?;
        db.seed_builtin_mark_types()?;
        Ok(db)
    }

    fn configure_connection(conn: &Connection, file_backed: bool) -> StorageResult<()> {
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        if file_backed {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn sqlite_version() -> &'static str {
        rusqlite::version()
    }

    pub(crate) fn lock_conn(&self) -> StorageResult<MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| StorageError::Integrity("database mutex poisoned".into()))
    }

    fn run_migrations(&self) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL,
                checksum TEXT NOT NULL
            );",
        )?;

        for m in MIGRATIONS {
            let canonical_sql = canonical_migration_sql(m.sql);
            let canonical_checksum = sha256_hex(canonical_sql.as_bytes());
            let legacy_crlf_checksum = sha256_hex(canonical_sql.replace('\n', "\r\n").as_bytes());
            let existing: Option<String> = conn
                .query_row(
                    "SELECT checksum FROM schema_migrations WHERE version = ?1",
                    params![m.version],
                    |row| row.get(0),
                )
                .ok();

            if let Some(prev_checksum) = existing {
                if prev_checksum == canonical_checksum {
                    continue;
                }
                if prev_checksum == legacy_crlf_checksum {
                    conn.execute(
                        "UPDATE schema_migrations SET checksum = ?1 WHERE version = ?2",
                        params![canonical_checksum, m.version],
                    )?;
                    tracing::warn!(
                        version = m.version,
                        name = m.name,
                        "normalized legacy CRLF migration checksum to canonical LF"
                    );
                    continue;
                }
                return Err(StorageError::Migration(format!(
                    "checksum mismatch for migration {} ({})",
                    m.version, m.name
                )));
            }

            let tx = conn.transaction()?;
            tx.execute_batch(&canonical_sql).map_err(|e| {
                StorageError::Migration(format!("migration {} ({}) failed: {e}", m.version, m.name))
            })?;
            tx.execute(
                "INSERT INTO schema_migrations (version, name, applied_at, checksum) VALUES (?1, ?2, ?3, ?4)",
                params![m.version, m.name, Utc::now().to_rfc3339(), canonical_checksum],
            )?;
            tx.commit()?;
            tracing::info!(
                version = m.version,
                name = m.name,
                "applied database migration"
            );
        }
        Ok(())
    }

    pub fn applied_migrations(&self) -> StorageResult<Vec<AppliedMigration>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT version, name, applied_at, checksum FROM schema_migrations ORDER BY version ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AppliedMigration {
                version: row.get(0)?,
                name: row.get(1)?,
                applied_at: row.get(2)?,
                checksum: row.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn seed_builtin_mark_types(&self) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        for mt in builtin_marks::all_builtins() {
            tx.execute(
                "INSERT OR IGNORE INTO mark_types (
                    id, profile_id, code, name, short_label, category, color, icon,
                    counts_in_average, numeric_value, is_builtin, sort_order
                ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)",
                params![
                    mt.id.to_string(),
                    mt.code,
                    mt.name,
                    mt.short_label,
                    mt.category.as_str(),
                    mt.color,
                    mt.icon,
                    if mt.counts_in_average { 1 } else { 0 },
                    mt.numeric_value.map(|d| d.to_string()),
                    mt.sort_order,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_sync_state(&self) -> StorageResult<SyncState> {
        let conn = self.lock_conn()?;
        conn.query_row(
            "SELECT revision, updated_at, last_source FROM sync_state WHERE id = 1",
            [],
            |row| {
                let rev: i64 = row.get(0)?;
                let ts_str: String = row.get(1)?;
                let src_str: String = row.get(2)?;
                Ok((rev, ts_str, src_str))
            },
        )
        .map_err(StorageError::from)
        .and_then(|(revision, ts_str, src_str)| {
            Ok(SyncState {
                revision,
                updated_at: crate::helpers::parse_dt(&ts_str)?,
                last_source: RecordSource::parse(&src_str).unwrap_or(RecordSource::Gui),
            })
        })
    }

    pub fn check_integrity(&self) -> StorageResult<bool> {
        let conn = self.lock_conn()?;
        let result: String = conn.pragma_query_value(None, "integrity_check", |r| r.get(0))?;
        Ok(result.eq_ignore_ascii_case("ok"))
    }

    pub fn verify_sqlite_file(path: impl AsRef<Path>) -> StorageResult<()> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(StorageError::Integrity(format!(
                "file does not exist: {}",
                path.display()
            )));
        }
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let res: String = conn.pragma_query_value(None, "integrity_check", |r| r.get(0))?;
        if !res.eq_ignore_ascii_case("ok") {
            return Err(StorageError::Integrity(format!(
                "integrity_check returned: {res}"
            )));
        }
        // Verify core Oxidium tables exist
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('profiles','subjects','grades','tasks','schema_migrations')",
            [],
            |r| r.get(0),
        )?;
        if count < 5 {
            return Err(StorageError::Integrity(
                "file is not a valid Oxidium database (missing required tables)".into(),
            ));
        }
        Ok(())
    }

    pub fn checkpoint_wal(&self) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }

    /// Creates a consistent online SQLite backup safe even while WAL transactions are active.
    pub fn backup_to_path(&self, dest: impl AsRef<Path>) -> StorageResult<()> {
        let dest = dest.as_ref();
        if let Some(parent) = dest.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        let src_conn = self.lock_conn()?;
        let mut dst_conn = Connection::open(dest)?;
        {
            let b = backup::Backup::new(&src_conn, &mut dst_conn)?;
            b.run_to_completion(50, Duration::from_millis(10), None)?;
        }
        drop(dst_conn);
        Self::verify_sqlite_file(dest)?;
        Ok(())
    }

    /// Safely restores this database from `source_path`:
    /// 1. Verifies integrity and Oxidium schema of `source_path`.
    /// 2. Creates a pre-restore safety backup in `pre_restore_backup_path`.
    /// 3. Copies `source_path` into live DB using SQLite online Backup API.
    /// 4. If copy or post-restore verification fails, automatically restores from the safety backup.
    pub fn restore_from_path(
        &self,
        source_path: impl AsRef<Path>,
        pre_restore_backup_path: impl AsRef<Path>,
    ) -> StorageResult<()> {
        let source_path = source_path.as_ref();
        let pre_backup = pre_restore_backup_path.as_ref();

        // Step 1: validate candidate before touching live DB
        Self::verify_sqlite_file(source_path)?;

        // Step 2: create safety backup of current live state
        self.backup_to_path(pre_backup)?;

        // Step 3: online restore into live connection
        let restore_attempt = (|| -> StorageResult<()> {
            let src_conn = Connection::open_with_flags(
                source_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let mut dst_conn = self.lock_conn()?;
            let b = backup::Backup::new(&src_conn, &mut dst_conn)?;
            b.run_to_completion(50, Duration::from_millis(10), None)?;
            Ok(())
        })();

        if let Err(err) = restore_attempt {
            tracing::error!(error = %err, "restore failed, rolling back to pre-restore backup");
            let _ = self.rollback_from_file(pre_backup);
            return Err(err);
        }

        // Step 4: verify restored DB and ensure migrations/sync_state are up to date
        match self.check_integrity() {
            Ok(true) => {
                self.run_migrations()?;
                self.seed_builtin_mark_types()?;
                let conn = self.lock_conn()?;
                crate::helpers::bump_sync_revision(&conn, RecordSource::Import)?;
                Ok(())
            }
            Ok(false) | Err(_) => {
                let _ = self.rollback_from_file(pre_backup);
                Err(StorageError::Integrity(
                    "restored database failed integrity check; previous state was restored".into(),
                ))
            }
        }
    }

    fn rollback_from_file(&self, safety_backup: &Path) -> StorageResult<()> {
        let src =
            Connection::open_with_flags(safety_backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut dst = self.lock_conn()?;
        let b = backup::Backup::new(&src, &mut dst)?;
        b.run_to_completion(50, Duration::from_millis(10), None)?;
        Ok(())
    }
}
