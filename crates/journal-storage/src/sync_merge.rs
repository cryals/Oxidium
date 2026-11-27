use chrono::{DateTime, Utc};
use journal_domain::{
    RecordSource, SearchEntityType, SyncConnectionStatus, SyncMergeReport, SyncRemoteConfig,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

use crate::db::Database;
use crate::error::{StorageError, StorageResult};
use crate::helpers::{
    parse_opt_date, parse_opt_dt, parse_opt_uuid, parse_uuid, upsert_search_entry,
};

const MIGRATION_0004_SQL: &str = include_str!("../../../migrations/0004_sync_remote_state.sql");

impl Database {
    pub fn load_sync_remote_config(&self) -> StorageResult<SyncRemoteConfig> {
        let conn = self.lock_conn()?;
        load_sync_remote_config_conn(&conn)
    }

    pub fn save_sync_remote_config(&self, cfg: &SyncRemoteConfig) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        save_sync_remote_config_conn(&conn, cfg)
    }

    pub fn reset_sync_remote_config(&self) -> StorageResult<()> {
        let cfg = SyncRemoteConfig::default();
        self.save_sync_remote_config(&cfg)
    }

    /// Returns true if the local database contains any user-authored academic data
    /// (more than 1 profile, any non-default edits, or any subject/period/grade/attendance/task/note).
    pub fn has_user_academic_data(&self) -> StorageResult<bool> {
        let conn = self.lock_conn()?;
        let count: i64 = conn.query_row(
            "SELECT
                (SELECT CASE WHEN COUNT(*) > 1 THEN 1 ELSE 0 END FROM profiles) +
                (SELECT CASE WHEN COUNT(*) > 1 THEN 1 ELSE 0 END FROM boards) +
                (SELECT COUNT(*) FROM subjects) +
                (SELECT COUNT(*) FROM academic_periods) +
                (SELECT COUNT(*) FROM grades) +
                (SELECT COUNT(*) FROM attendance) +
                (SELECT COUNT(*) FROM tasks)",
            [],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Restores a verified SQLite snapshot onto the live database while preserving
    /// this device's own `sync_remote_config` identity and sync progress cursors.
    pub fn restore_from_path_preserving_sync_state(
        &self,
        source_path: impl AsRef<Path>,
        new_synced_seq: i64,
        last_merge_summary: Option<String>,
    ) -> StorageResult<SyncRemoteConfig> {
        let source_path = source_path.as_ref();
        Self::verify_sqlite_file(source_path)?;

        let mut preserved = self.load_sync_remote_config()?;
        let src = Connection::open_with_flags(
            source_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        {
            let mut dst = self.lock_conn()?;
            {
                let backup = rusqlite::backup::Backup::new(&src, &mut dst)?;
                backup.run_to_completion(100, Duration::from_millis(5), None)?;
            }
            dst.pragma_update(None, "foreign_keys", "ON")?;
            dst.pragma_update(None, "journal_mode", "WAL")?;

            // Guarantee schema v4 tables/triggers exist even if pulled from older schema
            let has_v4: i64 = dst.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_remote_config'",
                [],
                |r| r.get(0),
            )?;
            if has_v4 == 0 {
                dst.execute_batch(MIGRATION_0004_SQL)?;
            }

            let new_local_rev: i64 = dst
                .query_row("SELECT revision FROM sync_state WHERE id = 1", [], |r| {
                    r.get(0)
                })
                .unwrap_or(1);
            preserved.last_synced_seq = new_synced_seq;
            preserved.last_synced_local_rev = new_local_rev;
            preserved.last_sync_at = Some(Utc::now());
            preserved.last_status = SyncConnectionStatus::Synced;
            preserved.last_error = None;
            if let Some(summary) = last_merge_summary {
                preserved.last_merge_summary = Some(summary);
            }
            save_sync_remote_config_conn(&dst, &preserved)?;
        }

        Ok(preserved)
    }

    /// Performs a deterministic, tombstone-aware, row-level 3-way SQLite merge between
    /// the local database and a decrypted remote SQLite snapshot file, validates SQLite
    /// `integrity_check` + `foreign_key_check`, rebuilds FTS5 `search_index`, and restores
    /// the merged database atomically onto `self` (preserving local sync identity).
    pub fn merge_remote_snapshot(
        &self,
        remote_db_path: impl AsRef<Path>,
        remote_seq: i64,
    ) -> StorageResult<SyncMergeReport> {
        let remote_db_path = remote_db_path.as_ref();
        Self::verify_sqlite_file(remote_db_path)?;

        let tmp = tempfile::tempdir()?;
        let local_copy_path = tmp.path().join("local-snap.db");
        let remote_copy_path = tmp.path().join("remote-snap.db");
        let merged_path = tmp.path().join("merged.db");

        self.backup_to_path(&local_copy_path)?;
        std::fs::copy(remote_db_path, &remote_copy_path)?;

        // Ensure remote copy has schema v4 (triggers/tombstones) if missing
        let local_db = Database::open(&local_copy_path)?;
        let remote_db = Database::open(&remote_copy_path)?;
        let merged_db = Database::open(&merged_path)?;

        let report = {
            let local_conn = local_db.lock_conn()?;
            let remote_conn = remote_db.lock_conn()?;
            let mut merged_conn = merged_db.lock_conn()?;
            merge_two_connections(&local_conn, &remote_conn, &mut merged_conn)?
        };

        // Ensure WAL is flushed into single self-contained file before restoring
        merged_db.checkpoint_wal()?;
        drop(merged_db);
        drop(remote_db);
        drop(local_db);

        self.restore_from_path_preserving_sync_state(
            &merged_path,
            remote_seq,
            Some(report.summary_ru.clone()),
        )?;

        Ok(report)
    }
}

fn load_sync_remote_config_conn(conn: &Connection) -> StorageResult<SyncRemoteConfig> {
    let row: Option<SyncRemoteConfig> = conn
        .query_row(
            "SELECT enabled, auto_sync, server_url, server_pub, server_fingerprint,
                    device_id, device_name, vault_id, last_synced_seq, last_synced_local_rev,
                    last_sync_at, last_status, last_error, batch_debounce_secs, last_merge_summary
             FROM sync_remote_config WHERE id = 1",
            [],
            |r| {
                let enabled: i64 = r.get(0)?;
                let auto_sync: i64 = r.get(1)?;
                let last_sync_at_raw: Option<String> = r.get(10)?;
                let last_status_raw: String = r.get(11)?;
                let debounce: i64 = r.get(13)?;
                Ok(SyncRemoteConfig {
                    enabled: enabled != 0,
                    auto_sync: auto_sync != 0,
                    server_url: r.get(2)?,
                    server_pub: r.get(3)?,
                    server_fingerprint: r.get(4)?,
                    device_id: r.get(5)?,
                    device_name: r.get(6)?,
                    vault_id: r.get(7)?,
                    last_synced_seq: r.get(8)?,
                    last_synced_local_rev: r.get(9)?,
                    last_sync_at: last_sync_at_raw
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|d| d.with_timezone(&Utc)),
                    last_status: SyncConnectionStatus::parse(&last_status_raw),
                    last_error: r.get(12)?,
                    batch_debounce_secs: debounce.max(2) as u64,
                    last_merge_summary: r.get(14)?,
                })
            },
        )
        .optional()?;
    Ok(row.unwrap_or_default())
}

fn save_sync_remote_config_conn(conn: &Connection, cfg: &SyncRemoteConfig) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO sync_remote_config (
            id, enabled, auto_sync, server_url, server_pub, server_fingerprint,
            device_id, device_name, vault_id, last_synced_seq, last_synced_local_rev,
            last_sync_at, last_status, last_error, batch_debounce_secs, last_merge_summary
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
            enabled = excluded.enabled,
            auto_sync = excluded.auto_sync,
            server_url = excluded.server_url,
            server_pub = excluded.server_pub,
            server_fingerprint = excluded.server_fingerprint,
            device_id = excluded.device_id,
            device_name = excluded.device_name,
            vault_id = excluded.vault_id,
            last_synced_seq = excluded.last_synced_seq,
            last_synced_local_rev = excluded.last_synced_local_rev,
            last_sync_at = excluded.last_sync_at,
            last_status = excluded.last_status,
            last_error = excluded.last_error,
            batch_debounce_secs = excluded.batch_debounce_secs,
            last_merge_summary = excluded.last_merge_summary",
        params![
            if cfg.enabled { 1 } else { 0 },
            if cfg.auto_sync { 1 } else { 0 },
            cfg.server_url,
            cfg.server_pub,
            cfg.server_fingerprint,
            cfg.device_id,
            cfg.device_name,
            cfg.vault_id,
            cfg.last_synced_seq,
            cfg.last_synced_local_rev,
            cfg.last_sync_at.map(|d| d.to_rfc3339()),
            cfg.last_status.as_str(),
            cfg.last_error,
            cfg.batch_debounce_secs.max(2) as i64,
            cfg.last_merge_summary,
        ],
    )?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct MergeRow {
    pk: String,
    ts: String,
    cols: Vec<rusqlite::types::Value>,
    from_remote: bool,
}

fn ts_ge(lhs: &str, rhs: &str) -> bool {
    match (
        DateTime::parse_from_rfc3339(lhs),
        DateTime::parse_from_rfc3339(rhs),
    ) {
        (Ok(a), Ok(b)) => a >= b,
        _ => lhs >= rhs,
    }
}

fn ts_cmp(lhs: &str, rhs: &str) -> std::cmp::Ordering {
    match (
        DateTime::parse_from_rfc3339(lhs),
        DateTime::parse_from_rfc3339(rhs),
    ) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => lhs.cmp(rhs),
    }
}

fn value_sort_key(vals: &[rusqlite::types::Value]) -> String {
    vals.iter()
        .map(|v| match v {
            rusqlite::types::Value::Null => "N".to_string(),
            rusqlite::types::Value::Integer(i) => format!("I:{i}"),
            rusqlite::types::Value::Real(f) => format!("R:{f:.6}"),
            rusqlite::types::Value::Text(s) => format!("T:{s}"),
            rusqlite::types::Value::Blob(b) => format!("B:{}", hex::encode(b)),
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn read_tombstones(conn: &Connection) -> StorageResult<HashMap<(String, String), String>> {
    let mut map = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT entity_table, entity_pk, deleted_at FROM sync_tombstones")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (tbl, pk, ts) = row?;
        map.entry((tbl, pk))
            .and_modify(|cur: &mut String| {
                if ts_cmp(&ts, cur) == std::cmp::Ordering::Greater {
                    *cur = ts.clone();
                }
            })
            .or_insert(ts);
    }
    Ok(map)
}

fn merge_tombstones(
    local: &Connection,
    remote: &Connection,
) -> StorageResult<HashMap<(String, String), String>> {
    let mut out = read_tombstones(local)?;
    for (k, v) in read_tombstones(remote)? {
        out.entry(k)
            .and_modify(|cur| {
                if ts_cmp(&v, cur) == std::cmp::Ordering::Greater {
                    *cur = v.clone();
                }
            })
            .or_insert(v);
    }
    Ok(out)
}

fn read_table_rows(
    conn: &Connection,
    sql: &str,
    col_count: usize,
    pk_idx: usize,
    ts_idx: Option<usize>,
    from_remote: bool,
) -> StorageResult<Vec<MergeRow>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        let mut cols = Vec::with_capacity(col_count);
        for i in 0..col_count {
            cols.push(r.get::<_, rusqlite::types::Value>(i)?);
        }
        let pk = match &cols[pk_idx] {
            rusqlite::types::Value::Text(s) => s.clone(),
            rusqlite::types::Value::Integer(i) => i.to_string(),
            _ => String::new(),
        };
        let ts = match ts_idx.and_then(|idx| cols.get(idx)) {
            Some(rusqlite::types::Value::Text(s)) => s.clone(),
            _ => "1970-01-01T00:00:00Z".to_string(),
        };
        Ok(MergeRow {
            pk,
            ts,
            cols,
            from_remote,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn lww_merge_rows(
    local_rows: Vec<MergeRow>,
    remote_rows: Vec<MergeRow>,
    table: &str,
    tombstones: &HashMap<(String, String), String>,
    tombstones_applied: &mut usize,
) -> BTreeMap<String, MergeRow> {
    let mut candidates: BTreeMap<String, MergeRow> = BTreeMap::new();
    for row in local_rows.into_iter().chain(remote_rows) {
        candidates
            .entry(row.pk.clone())
            .and_modify(|existing| {
                let ord = ts_cmp(&row.ts, &existing.ts);
                let take_new = match ord {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Less => false,
                    std::cmp::Ordering::Equal => {
                        value_sort_key(&row.cols) > value_sort_key(&existing.cols)
                    }
                };
                if take_new {
                    *existing = row.clone();
                }
            })
            .or_insert(row);
    }

    let mut kept = BTreeMap::new();
    for (pk, row) in candidates {
        if let Some(deleted_at) = tombstones.get(&(table.to_string(), pk.clone()))
            && ts_ge(deleted_at, &row.ts)
        {
            *tombstones_applied += 1;
            continue;
        }
        kept.insert(pk, row);
    }
    kept
}

fn val_str(v: &rusqlite::types::Value) -> Option<String> {
    match v {
        rusqlite::types::Value::Text(s) => Some(s.clone()),
        _ => None,
    }
}

fn val_i64(v: &rusqlite::types::Value) -> i64 {
    match v {
        rusqlite::types::Value::Integer(i) => *i,
        _ => 0,
    }
}

fn insert_row_dyn(
    conn: &Connection,
    sql: &str,
    cols: &[rusqlite::types::Value],
) -> StorageResult<()> {
    let params_vec: Vec<&dyn rusqlite::ToSql> =
        cols.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    conn.execute(sql, params_vec.as_slice())?;
    Ok(())
}

fn merge_two_connections(
    local: &Connection,
    remote: &Connection,
    merged: &mut Connection,
) -> StorageResult<SyncMergeReport> {
    let tombstones = merge_tombstones(local, remote)?;
    let mut tombstones_applied = 0usize;
    let mut deduplicated_items = 0usize;
    let mut records_from_remote = 0usize;

    merged.pragma_update(None, "foreign_keys", "OFF")?;
    let tx = merged.transaction()?;

    // Wipe seeded mark_types & defaults so we control every row deterministically
    tx.execute("DELETE FROM search_index", [])?;
    tx.execute("DELETE FROM sync_tombstones", [])?;
    tx.execute("DELETE FROM mark_types", [])?;
    tx.execute("DELETE FROM settings", [])?;

    // 1. profiles (11 cols: id=0 .. updated_at=10)
    let prof_sql = "SELECT id, name, icon, color, institution, group_name, academic_year, description, is_archived, created_at, updated_at FROM profiles";
    let merged_profiles = lww_merge_rows(
        read_table_rows(local, prof_sql, 11, 0, Some(10), false)?,
        read_table_rows(remote, prof_sql, 11, 0, Some(10), true)?,
        "profiles",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_profiles = HashSet::new();
    let mut active_candidate_profiles = Vec::new();
    for (id, row) in &merged_profiles {
        kept_profiles.insert(id.clone());
        if val_i64(&row.cols[8]) == 0 {
            active_candidate_profiles.push(id.clone());
        }
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO profiles (id, name, icon, color, institution, group_name, academic_year, description, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            &row.cols,
        )?;
    }

    // 2. academic_periods (9 cols: id=0, profile_id=1 .. updated_at=8)
    let per_sql = "SELECT id, profile_id, name, period_type, start_date, end_date, is_archived, created_at, updated_at FROM academic_periods";
    let merged_periods = lww_merge_rows(
        read_table_rows(local, per_sql, 9, 0, Some(8), false)?,
        read_table_rows(remote, per_sql, 9, 0, Some(8), true)?,
        "academic_periods",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_periods = HashSet::new();
    for (id, row) in &merged_periods {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_profiles.contains(&pid) {
            continue;
        }
        kept_periods.insert(id.clone());
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO academic_periods (id, profile_id, name, period_type, start_date, end_date, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            &row.cols,
        )?;
    }

    // 3. subjects (14 cols: id=0, profile_id=1, academic_period_id=2 .. updated_at=13)
    let subj_sql = "SELECT id, profile_id, academic_period_id, name, short_name, teacher, color, icon, description, sort_order, is_archived, target_average, created_at, updated_at FROM subjects";
    let merged_subjects = lww_merge_rows(
        read_table_rows(local, subj_sql, 14, 0, Some(13), false)?,
        read_table_rows(remote, subj_sql, 14, 0, Some(13), true)?,
        "subjects",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_subjects = HashSet::new();
    let mut subject_rows_kept = Vec::new();
    for (id, mut row) in merged_subjects {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_profiles.contains(&pid) {
            continue;
        }
        if let Some(apid) = val_str(&row.cols[2])
            && !kept_periods.contains(&apid)
        {
            row.cols[2] = rusqlite::types::Value::Null;
        }
        kept_subjects.insert(id);
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO subjects (id, profile_id, academic_period_id, name, short_name, teacher, color, icon, description, sort_order, is_archived, target_average, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            &row.cols,
        )?;
        subject_rows_kept.push(row);
    }

    // 4. mark_types (12 cols: id=0, profile_id=1, code=2, short_label=4, is_builtin=10)
    let mt_sql = "SELECT id, profile_id, code, name, short_label, category, color, icon, counts_in_average, numeric_value, is_builtin, sort_order FROM mark_types";
    let mut all_mts = read_table_rows(local, mt_sql, 12, 0, None, false)?;
    all_mts.extend(read_table_rows(remote, mt_sql, 12, 0, None, true)?);
    let mut mt_by_key: BTreeMap<(Option<String>, String), MergeRow> = BTreeMap::new();
    let mut mt_remap: HashMap<String, String> = HashMap::new();
    for row in all_mts {
        if tombstones.contains_key(&("mark_types".to_string(), row.pk.clone())) {
            tombstones_applied += 1;
            continue;
        }
        let pid = val_str(&row.cols[1]);
        if let Some(ref p) = pid
            && !kept_profiles.contains(p)
        {
            continue;
        }
        let code = val_str(&row.cols[2]).unwrap_or_default();
        let ukey = (pid, code);
        match mt_by_key.get_mut(&ukey) {
            Some(winner) => {
                if row.pk != winner.pk {
                    // Deterministic winner: builtin first, else smaller UUID
                    let row_builtin = val_i64(&row.cols[10]);
                    let win_builtin = val_i64(&winner.cols[10]);
                    let replace = (row_builtin > win_builtin)
                        || (row_builtin == win_builtin && row.pk < winner.pk);
                    if replace {
                        mt_remap.insert(winner.pk.clone(), row.pk.clone());
                        *winner = row;
                    } else {
                        mt_remap.insert(row.pk.clone(), winner.pk.clone());
                    }
                    deduplicated_items += 1;
                }
            }
            None => {
                mt_by_key.insert(ukey, row);
            }
        }
    }
    let mut kept_mark_types = HashMap::new();
    for row in mt_by_key.into_values() {
        let short_label = val_str(&row.cols[4]).unwrap_or_default();
        kept_mark_types.insert(row.pk.clone(), short_label);
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO mark_types (id, profile_id, code, name, short_label, category, color, icon, counts_in_average, numeric_value, is_builtin, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            &row.cols,
        )?;
    }

    // 5. grades (16 cols: id=0, profile_id=1, subject_id=2, date=3, mark_type_id=5 .. topic=9, comment=10 .. updated_at=15)
    let grades_sql = "SELECT id, profile_id, subject_id, date, time, mark_type_id, numeric_value, weight, work_type, topic, comment, teacher, source, created_at, updated_at FROM grades";
    let merged_grades = lww_merge_rows(
        read_table_rows(local, grades_sql, 15, 0, Some(14), false)?,
        read_table_rows(remote, grades_sql, 15, 0, Some(14), true)?,
        "grades",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_grades = HashSet::new();
    let mut grade_rows_kept = Vec::new();
    for (id, mut row) in merged_grades {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        let Some(sid) = val_str(&row.cols[2]) else {
            continue;
        };
        let mt_orig = val_str(&row.cols[5]).unwrap_or_default();
        let mt_id = mt_remap.get(&mt_orig).cloned().unwrap_or(mt_orig);
        row.cols[5] = rusqlite::types::Value::Text(mt_id.clone());
        if !kept_profiles.contains(&pid)
            || !kept_subjects.contains(&sid)
            || !kept_mark_types.contains_key(&mt_id)
        {
            continue;
        }
        kept_grades.insert(id);
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO grades (id, profile_id, subject_id, date, time, mark_type_id, numeric_value, weight, work_type, topic, comment, teacher, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            &row.cols,
        )?;
        grade_rows_kept.push(row);
    }

    // 6. attendance (12 cols: id=0, profile_id=1, subject_id=2, date=3, status=5, mark_type_id=6, reason=7, comment=8 .. updated_at=11)
    let att_sql = "SELECT id, profile_id, subject_id, date, time, status, mark_type_id, reason, comment, source, created_at, updated_at FROM attendance";
    let merged_att = lww_merge_rows(
        read_table_rows(local, att_sql, 12, 0, Some(11), false)?,
        read_table_rows(remote, att_sql, 12, 0, Some(11), true)?,
        "attendance",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut att_rows_kept = Vec::new();
    for (_, mut row) in merged_att {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        let Some(sid) = val_str(&row.cols[2]) else {
            continue;
        };
        let mt_orig = val_str(&row.cols[6]).unwrap_or_default();
        let mt_id = mt_remap.get(&mt_orig).cloned().unwrap_or(mt_orig);
        row.cols[6] = rusqlite::types::Value::Text(mt_id.clone());
        if !kept_profiles.contains(&pid)
            || !kept_subjects.contains(&sid)
            || !kept_mark_types.contains_key(&mt_id)
        {
            continue;
        }
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO attendance (id, profile_id, subject_id, date, time, status, mark_type_id, reason, comment, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            &row.cols,
        )?;
        att_rows_kept.push(row);
    }

    // 7. boards (9 cols: id=0, profile_id=1 .. updated_at=8)
    let boards_sql = "SELECT id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at FROM boards";
    let merged_boards = lww_merge_rows(
        read_table_rows(local, boards_sql, 9, 0, Some(8), false)?,
        read_table_rows(remote, boards_sql, 9, 0, Some(8), true)?,
        "boards",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_boards = HashSet::new();
    for (id, row) in &merged_boards {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_profiles.contains(&pid) {
            continue;
        }
        kept_boards.insert(id.clone());
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO boards (id, profile_id, name, description, color, sort_order, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            &row.cols,
        )?;
    }

    // 8. kanban_columns (11 cols: id=0, board_id=1 .. updated_at=10)
    let cols_sql = "SELECT id, board_id, name, color, sort_order, is_collapsed, is_archived, wip_limit, is_completed_column, created_at, updated_at FROM kanban_columns";
    let merged_cols = lww_merge_rows(
        read_table_rows(local, cols_sql, 11, 0, Some(10), false)?,
        read_table_rows(remote, cols_sql, 11, 0, Some(10), true)?,
        "kanban_columns",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_columns = HashSet::new();
    for (id, row) in &merged_cols {
        let Some(bid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_boards.contains(&bid) {
            continue;
        }
        kept_columns.insert(id.clone());
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO kanban_columns (id, board_id, name, color, sort_order, is_collapsed, is_archived, wip_limit, is_completed_column, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            &row.cols,
        )?;
    }

    // 9. tasks (17 cols: id=0, profile_id=1, board_id=2, column_id=3, subject_id=4, title=5 .. updated_at=16)
    let tasks_sql = "SELECT id, profile_id, board_id, column_id, subject_id, title, description, priority, color, start_date, due_at, completed_at, notes, sort_order, is_archived, created_at, updated_at FROM tasks";
    let merged_tasks = lww_merge_rows(
        read_table_rows(local, tasks_sql, 17, 0, Some(16), false)?,
        read_table_rows(remote, tasks_sql, 17, 0, Some(16), true)?,
        "tasks",
        &tombstones,
        &mut tombstones_applied,
    );
    let mut kept_tasks: HashMap<String, String> = HashMap::new();
    let mut task_rows_kept = Vec::new();
    for (id, mut row) in merged_tasks {
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        let Some(bid) = val_str(&row.cols[2]) else {
            continue;
        };
        let Some(cid) = val_str(&row.cols[3]) else {
            continue;
        };
        if !kept_profiles.contains(&pid)
            || !kept_boards.contains(&bid)
            || !kept_columns.contains(&cid)
        {
            continue;
        }
        if let Some(sid) = val_str(&row.cols[4])
            && !kept_subjects.contains(&sid)
        {
            row.cols[4] = rusqlite::types::Value::Null;
        }
        kept_tasks.insert(id, row.ts.clone());
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO tasks (id, profile_id, board_id, column_id, subject_id, title, description, priority, color, start_date, due_at, completed_at, notes, sort_order, is_archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            &row.cols,
        )?;
        task_rows_kept.push(row);
    }

    // 10. tags & 11. badges with unique (profile_id, name_normalized) deduplication
    let tags_res = merge_unique_taxonomy(
        local,
        remote,
        &tx,
        "tags",
        "SELECT id, profile_id, name, name_normalized, color, icon, description FROM tags",
        "INSERT INTO tags (id, profile_id, name, name_normalized, color, icon, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        &kept_profiles,
        &tombstones,
        &mut tombstones_applied,
        &mut deduplicated_items,
        &mut records_from_remote,
    )?;
    let (kept_tags, tag_remap, tag_rows_kept) = (tags_res.kept_ids, tags_res.remap, tags_res.rows);

    let badges_res = merge_unique_taxonomy(
        local,
        remote,
        &tx,
        "badges",
        "SELECT id, profile_id, name, name_normalized, color, icon, description FROM badges",
        "INSERT INTO badges (id, profile_id, name, name_normalized, color, icon, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        &kept_profiles,
        &tombstones,
        &mut tombstones_applied,
        &mut deduplicated_items,
        &mut records_from_remote,
    )?;
    let (kept_badges, badge_remap, badge_rows_kept) =
        (badges_res.kept_ids, badges_res.remap, badges_res.rows);

    // 12. task_tags & 13. task_badges
    merge_task_join(
        local,
        remote,
        &tx,
        "task_tags",
        "SELECT task_id, tag_id FROM task_tags",
        "INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?1, ?2)",
        &kept_tasks,
        &kept_tags,
        &tag_remap,
        &tombstones,
        &mut tombstones_applied,
        &mut records_from_remote,
    )?;
    merge_task_join(
        local,
        remote,
        &tx,
        "task_badges",
        "SELECT task_id, badge_id FROM task_badges",
        "INSERT OR IGNORE INTO task_badges (task_id, badge_id) VALUES (?1, ?2)",
        &kept_tasks,
        &kept_badges,
        &badge_remap,
        &tombstones,
        &mut tombstones_applied,
        &mut records_from_remote,
    )?;

    // 14. task_checklist_items (7 cols: id=0, task_id=1 .. updated_at=6)
    let cl_sql = "SELECT id, task_id, title, is_completed, sort_order, created_at, updated_at FROM task_checklist_items";
    let merged_cl = lww_merge_rows(
        read_table_rows(local, cl_sql, 7, 0, Some(6), false)?,
        read_table_rows(remote, cl_sql, 7, 0, Some(6), true)?,
        "task_checklist_items",
        &tombstones,
        &mut tombstones_applied,
    );
    for (_, row) in merged_cl {
        let Some(tid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_tasks.contains_key(&tid) {
            continue;
        }
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO task_checklist_items (id, task_id, title, is_completed, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &row.cols,
        )?;
    }

    // 15. task_grade_links (3 cols: task_id=0, grade_id=1, created_at=2)
    let tgl_sql = "SELECT task_id, grade_id, created_at FROM task_grade_links";
    let mut all_tgl = read_table_rows(local, tgl_sql, 3, 0, Some(2), false)?;
    all_tgl.extend(read_table_rows(remote, tgl_sql, 3, 0, Some(2), true)?);
    let mut seen_tgl = HashSet::new();
    for row in all_tgl {
        let Some(tid) = val_str(&row.cols[0]) else {
            continue;
        };
        let Some(gid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_tasks.contains_key(&tid) || !kept_grades.contains(&gid) {
            continue;
        }
        let ckey = format!("{tid}|{gid}");
        if let Some(del_ts) = tombstones.get(&("task_grade_links".to_string(), ckey.clone()))
            && ts_ge(del_ts, &row.ts)
        {
            tombstones_applied += 1;
            continue;
        }
        if seen_tgl.insert(ckey) {
            if row.from_remote {
                records_from_remote += 1;
            }
            insert_row_dyn(
                &tx,
                "INSERT INTO task_grade_links (task_id, grade_id, created_at) VALUES (?1, ?2, ?3)",
                &row.cols,
            )?;
        }
    }

    // 16. notifications (9 cols: id=0, profile_id=1 .. is_read=6, is_dismissed=7, created_at=8)
    let notif_sql = "SELECT id, profile_id, kind, title, body, entity_type, entity_id, is_read, is_dismissed, created_at FROM notifications";
    let mut all_notifs = read_table_rows(local, notif_sql, 10, 0, Some(9), false)?;
    all_notifs.extend(read_table_rows(remote, notif_sql, 10, 0, Some(9), true)?);
    let mut notifs_map: BTreeMap<String, MergeRow> = BTreeMap::new();
    for mut row in all_notifs {
        if let Some(del_ts) = tombstones.get(&("notifications".to_string(), row.pk.clone()))
            && ts_ge(del_ts, &row.ts)
        {
            tombstones_applied += 1;
            continue;
        }
        if let Some(pid) = val_str(&row.cols[1])
            && !kept_profiles.contains(&pid)
        {
            row.cols[1] = rusqlite::types::Value::Null;
        }
        notifs_map
            .entry(row.pk.clone())
            .and_modify(|cur| {
                let r = if val_i64(&cur.cols[7]) != 0 || val_i64(&row.cols[7]) != 0 {
                    1
                } else {
                    0
                };
                let d = if val_i64(&cur.cols[8]) != 0 || val_i64(&row.cols[8]) != 0 {
                    1
                } else {
                    0
                };
                cur.cols[7] = rusqlite::types::Value::Integer(r);
                cur.cols[8] = rusqlite::types::Value::Integer(d);
            })
            .or_insert(row);
    }
    for row in notifs_map.into_values() {
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO notifications (id, profile_id, kind, title, body, entity_type, entity_id, is_read, is_dismissed, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            &row.cols,
        )?;
    }

    // 17. reminders (5 cols: id=0, task_id=1, remind_at=2, offset_minutes=3, is_fired=4, created_at=5)
    let rem_sql =
        "SELECT id, task_id, remind_at, offset_minutes, is_fired, created_at FROM reminders";
    let mut all_rems = read_table_rows(local, rem_sql, 6, 0, Some(5), false)?;
    all_rems.extend(read_table_rows(remote, rem_sql, 6, 0, Some(5), true)?);
    let mut rems_map: BTreeMap<String, MergeRow> = BTreeMap::new();
    for row in all_rems {
        let Some(tid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_tasks.contains_key(&tid) {
            continue;
        }
        if let Some(del_ts) = tombstones.get(&("reminders".to_string(), row.pk.clone()))
            && ts_ge(del_ts, &row.ts)
        {
            tombstones_applied += 1;
            continue;
        }
        rems_map
            .entry(row.pk.clone())
            .and_modify(|cur| {
                if val_i64(&row.cols[4]) != 0 {
                    cur.cols[4] = rusqlite::types::Value::Integer(1);
                }
            })
            .or_insert(row);
    }
    for row in rems_map.into_values() {
        if row.from_remote {
            records_from_remote += 1;
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO reminders (id, task_id, remind_at, offset_minutes, is_fired, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &row.cols,
        )?;
    }

    // 18. settings (key=0, value_json=1, updated_at=2) + ensure active_profile_id valid
    let set_sql = "SELECT key, value_json, updated_at FROM settings";
    let mut all_sets = read_table_rows(local, set_sql, 3, 0, Some(2), false)?;
    all_sets.extend(read_table_rows(remote, set_sql, 3, 0, Some(2), true)?);
    let mut set_map: BTreeMap<String, MergeRow> = BTreeMap::new();
    for row in all_sets {
        set_map
            .entry(row.pk.clone())
            .and_modify(|cur| {
                let ord = ts_cmp(&row.ts, &cur.ts);
                let replace = match ord {
                    std::cmp::Ordering::Greater => true,
                    std::cmp::Ordering::Less => false,
                    std::cmp::Ordering::Equal => {
                        value_sort_key(&row.cols) > value_sort_key(&cur.cols)
                    }
                };
                if replace {
                    *cur = row.clone();
                }
            })
            .or_insert(row);
    }
    for (k, mut row) in set_map {
        if k == "app_settings"
            && let Some(json_str) = val_str(&row.cols[1])
            && let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&json_str)
        {
            let current_active = parsed
                .get("active_profile_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let active_valid = current_active
                .as_ref()
                .is_some_and(|id| kept_profiles.contains(id));
            if !active_valid {
                let fallback = active_candidate_profiles
                    .first()
                    .or_else(|| kept_profiles.iter().next())
                    .cloned();
                parsed["active_profile_id"] = match fallback {
                    Some(id) => serde_json::Value::String(id),
                    None => serde_json::Value::Null,
                };
                if let Ok(updated_json) = serde_json::to_string(&parsed) {
                    row.cols[1] = rusqlite::types::Value::Text(updated_json);
                }
            }
        }
        insert_row_dyn(
            &tx,
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)",
            &row.cols,
        )?;
    }

    // 19. activity_log (union by id, monotonic undone flag, retain recent 1000)
    let act_sql = "SELECT id, timestamp, client_identity, tool_name, profile_id, entity_type, entity_id, operation_type, summary, result_status, source, correlation_id, undo_payload_json, undone FROM activity_log";
    let mut all_acts = read_table_rows(local, act_sql, 14, 0, Some(1), false)?;
    all_acts.extend(read_table_rows(remote, act_sql, 14, 0, Some(1), true)?);
    let mut act_map: HashMap<String, MergeRow> = HashMap::new();
    for row in all_acts {
        act_map
            .entry(row.pk.clone())
            .and_modify(|cur| {
                if val_i64(&row.cols[13]) != 0 {
                    cur.cols[13] = rusqlite::types::Value::Integer(1);
                }
            })
            .or_insert(row);
    }
    let mut act_vec: Vec<MergeRow> = act_map.into_values().collect();
    act_vec.sort_by(|a, b| ts_cmp(&b.ts, &a.ts));
    for row in act_vec.into_iter().take(1000) {
        insert_row_dyn(
            &tx,
            "INSERT INTO activity_log (id, timestamp, client_identity, tool_name, profile_id, entity_type, entity_id, operation_type, summary, result_status, source, correlation_id, undo_payload_json, undone)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            &row.cols,
        )?;
    }

    // Restore merged tombstones (preserving delete history across offline devices)
    for ((tbl, pk), deleted_at) in &tombstones {
        tx.execute(
            "INSERT OR REPLACE INTO sync_tombstones (entity_table, entity_pk, deleted_at)
             VALUES (?1, ?2, ?3)",
            params![tbl, pk, deleted_at],
        )?;
    }

    // Remove tombstones overridden by surviving newer rows
    for id in &kept_profiles {
        tx.execute(
            "DELETE FROM sync_tombstones WHERE entity_table='profiles' AND entity_pk=?1",
            params![id],
        )?;
    }
    for sid in &kept_subjects {
        tx.execute(
            "DELETE FROM sync_tombstones WHERE entity_table='subjects' AND entity_pk=?1",
            params![sid],
        )?;
    }
    for gid in &kept_grades {
        tx.execute(
            "DELETE FROM sync_tombstones WHERE entity_table='grades' AND entity_pk=?1",
            params![gid],
        )?;
    }
    for tid in kept_tasks.keys() {
        tx.execute(
            "DELETE FROM sync_tombstones WHERE entity_table='tasks' AND entity_pk=?1",
            params![tid],
        )?;
    }

    // Update sync_state revision
    let max_local_rev: i64 = local
        .query_row("SELECT revision FROM sync_state WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(1);
    let max_remote_rev: i64 = remote
        .query_row("SELECT revision FROM sync_state WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(1);
    let next_rev = max_local_rev.max(max_remote_rev) + 1;
    tx.execute(
        "UPDATE sync_state SET revision = ?1, updated_at = ?2, last_source = ?3 WHERE id = 1",
        params![
            next_rev,
            Utc::now().to_rfc3339(),
            RecordSource::Import.as_str()
        ],
    )?;

    // Rebuild FTS5 search_index cleanly from merged entities
    for row in &subject_rows_kept {
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let name = val_str(&row.cols[3]).unwrap_or_default();
        let short_name = val_str(&row.cols[4]).unwrap_or_default();
        let teacher = val_str(&row.cols[5]).unwrap_or_default();
        let desc = val_str(&row.cols[8]).unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Subject,
            id,
            pid,
            Some(id),
            None,
            &name,
            &format!("{short_name} {teacher} {desc}"),
        )?;
    }
    for row in &grade_rows_kept {
        let topic = val_str(&row.cols[9]).unwrap_or_default();
        let comment = val_str(&row.cols[10]).unwrap_or_default();
        if topic.is_empty() && comment.is_empty() {
            continue;
        }
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let sid = parse_uuid(&val_str(&row.cols[2]).unwrap_or_default())?;
        let date = parse_opt_date(val_str(&row.cols[3]))?;
        let mt_id = val_str(&row.cols[5]).unwrap_or_default();
        let label = kept_mark_types.get(&mt_id).cloned().unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Grade,
            id,
            pid,
            Some(sid),
            date,
            if topic.is_empty() { &label } else { &topic },
            &format!("{label} {topic} {comment}"),
        )?;
    }
    for row in &att_rows_kept {
        let reason = val_str(&row.cols[7]).unwrap_or_default();
        let comment = val_str(&row.cols[8]).unwrap_or_default();
        if reason.is_empty() && comment.is_empty() {
            continue;
        }
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let sid = parse_uuid(&val_str(&row.cols[2]).unwrap_or_default())?;
        let date = parse_opt_date(val_str(&row.cols[3]))?;
        let status = val_str(&row.cols[5]).unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Attendance,
            id,
            pid,
            Some(sid),
            date,
            &status,
            &format!("{reason} {comment}"),
        )?;
    }
    for row in &task_rows_kept {
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let sid = parse_opt_uuid(val_str(&row.cols[4]))?;
        let due_date = parse_opt_dt(val_str(&row.cols[10]))?.map(|dt| dt.date_naive());
        let title = val_str(&row.cols[5]).unwrap_or_default();
        let desc = val_str(&row.cols[6]).unwrap_or_default();
        let notes = val_str(&row.cols[12]).unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Task,
            id,
            pid,
            sid,
            due_date,
            &title,
            &format!("{desc} {notes}"),
        )?;
    }
    for row in &tag_rows_kept {
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let name = val_str(&row.cols[2]).unwrap_or_default();
        let desc = val_str(&row.cols[6]).unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Tag,
            id,
            pid,
            None,
            None,
            &name,
            &desc,
        )?;
    }
    for row in &badge_rows_kept {
        let id = parse_uuid(&row.pk)?;
        let pid = parse_uuid(&val_str(&row.cols[1]).unwrap_or_default())?;
        let name = val_str(&row.cols[2]).unwrap_or_default();
        let desc = val_str(&row.cols[6]).unwrap_or_default();
        upsert_search_entry(
            &tx,
            SearchEntityType::Badge,
            id,
            pid,
            None,
            None,
            &name,
            &desc,
        )?;
    }

    tx.commit()?;
    merged.pragma_update(None, "foreign_keys", "ON")?;

    let ic: String = merged.pragma_query_value(None, "integrity_check", |r| r.get(0))?;
    if !ic.eq_ignore_ascii_case("ok") {
        return Err(StorageError::Integrity(format!(
            "merged database failed integrity_check: {ic}"
        )));
    }
    let fk_violations: usize = {
        let mut stmt = merged.prepare("PRAGMA foreign_key_check")?;
        let mut rows = stmt.query([])?;
        let mut cnt = 0usize;
        while rows.next()?.is_some() {
            cnt += 1;
        }
        cnt
    };
    if fk_violations > 0 {
        return Err(StorageError::Integrity(format!(
            "merged database failed foreign_key_check ({fk_violations} violation(s))"
        )));
    }

    let _ = Uuid::nil();
    let summary_ru = format!(
        "3-way слияние: +{} записей с ноды, применено удалений: {}, дедуплицировано: {}",
        records_from_remote, tombstones_applied, deduplicated_items
    );

    Ok(SyncMergeReport {
        merged_tables: 19,
        records_from_remote,
        tombstones_applied,
        deduplicated_items,
        summary_ru,
    })
}

struct TaxonomyMergeResult {
    kept_ids: HashSet<String>,
    remap: HashMap<String, String>,
    rows: Vec<MergeRow>,
}

#[allow(clippy::too_many_arguments)]
fn merge_unique_taxonomy(
    local: &Connection,
    remote: &Connection,
    tx: &Connection,
    table: &str,
    select_sql: &str,
    insert_sql: &str,
    kept_profiles: &HashSet<String>,
    tombstones: &HashMap<(String, String), String>,
    tombstones_applied: &mut usize,
    deduplicated_items: &mut usize,
    records_from_remote: &mut usize,
) -> StorageResult<TaxonomyMergeResult> {
    let mut all = read_table_rows(local, select_sql, 7, 0, None, false)?;
    all.extend(read_table_rows(remote, select_sql, 7, 0, None, true)?);
    let mut by_ukey: BTreeMap<(String, String), MergeRow> = BTreeMap::new();
    let mut remap = HashMap::new();

    for row in all {
        if tombstones.contains_key(&(table.to_string(), row.pk.clone())) {
            *tombstones_applied += 1;
            continue;
        }
        let Some(pid) = val_str(&row.cols[1]) else {
            continue;
        };
        if !kept_profiles.contains(&pid) {
            continue;
        }
        let norm = val_str(&row.cols[3]).unwrap_or_default();
        match by_ukey.get_mut(&(pid.clone(), norm.clone())) {
            Some(winner) => {
                if row.pk != winner.pk {
                    if row.pk < winner.pk {
                        remap.insert(winner.pk.clone(), row.pk.clone());
                        *winner = row;
                    } else {
                        remap.insert(row.pk.clone(), winner.pk.clone());
                    }
                    *deduplicated_items += 1;
                }
            }
            None => {
                by_ukey.insert((pid, norm), row);
            }
        }
    }

    let mut kept_ids = HashSet::new();
    let mut rows_out = Vec::new();
    for row in by_ukey.into_values() {
        kept_ids.insert(row.pk.clone());
        if row.from_remote {
            *records_from_remote += 1;
        }
        insert_row_dyn(tx, insert_sql, &row.cols)?;
        rows_out.push(row);
    }
    Ok(TaxonomyMergeResult {
        kept_ids,
        remap,
        rows: rows_out,
    })
}

#[allow(clippy::too_many_arguments)]
fn merge_task_join(
    local: &Connection,
    remote: &Connection,
    tx: &Connection,
    table: &str,
    select_sql: &str,
    insert_sql: &str,
    kept_tasks: &HashMap<String, String>,
    kept_targets: &HashSet<String>,
    target_remap: &HashMap<String, String>,
    tombstones: &HashMap<(String, String), String>,
    tombstones_applied: &mut usize,
    records_from_remote: &mut usize,
) -> StorageResult<()> {
    let mut all = read_table_rows(local, select_sql, 2, 0, None, false)?;
    all.extend(read_table_rows(remote, select_sql, 2, 0, None, true)?);
    let mut seen = HashSet::new();

    for mut row in all {
        let Some(tid) = val_str(&row.cols[0]) else {
            continue;
        };
        let orig_target = val_str(&row.cols[1]).unwrap_or_default();
        let target_id = target_remap
            .get(&orig_target)
            .cloned()
            .unwrap_or_else(|| orig_target.clone());
        row.cols[1] = rusqlite::types::Value::Text(target_id.clone());

        let Some(task_updated_at) = kept_tasks.get(&tid) else {
            continue;
        };
        if !kept_targets.contains(&target_id) {
            continue;
        }

        let pk_orig = format!("{tid}|{orig_target}");
        let pk_mapped = format!("{tid}|{target_id}");
        let tomb = tombstones
            .get(&(table.to_string(), pk_orig))
            .or_else(|| tombstones.get(&(table.to_string(), pk_mapped.clone())));
        if let Some(del_ts) = tomb
            && ts_ge(del_ts, task_updated_at)
        {
            *tombstones_applied += 1;
            continue;
        }

        if seen.insert(pk_mapped) {
            if row.from_remote {
                *records_from_remote += 1;
            }
            insert_row_dyn(tx, insert_sql, &row.cols)?;
        }
    }
    Ok(())
}
