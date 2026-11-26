use chrono::{DateTime, NaiveDate, Utc};
use journal_domain::{
    ActivityLogEntry, NotificationItem, NotificationKind, RecordSource, Reminder, SearchEntityType,
    SearchResultItem,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::Database;
use crate::error::StorageResult;
use crate::helpers::{
    bump_sync_revision, normalize_unicode, parse_dt, parse_opt_date, parse_opt_uuid, parse_uuid,
};

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub profile_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub entity_types: Vec<SearchEntityType>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteImpact {
    pub subjects: usize,
    pub grades: usize,
    pub attendance: usize,
    pub tasks: usize,
}

#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub key: String,
    pub tool_name: String,
    pub request_hash: String,
    pub response_json: String,
}

impl Database {
    // ========================================================================
    // Delete Impact Summaries (Safe Warnings Before Deletion)
    // ========================================================================

    pub fn subject_delete_impact(&self, subject_id: Uuid) -> StorageResult<DeleteImpact> {
        let conn = self.lock_conn()?;
        let sid = subject_id.to_string();
        let grades: i64 = conn.query_row(
            "SELECT COUNT(*) FROM grades WHERE subject_id = ?1",
            params![sid],
            |r| r.get(0),
        )?;
        let attendance: i64 = conn.query_row(
            "SELECT COUNT(*) FROM attendance WHERE subject_id = ?1",
            params![sid],
            |r| r.get(0),
        )?;
        let tasks: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE subject_id = ?1",
            params![sid],
            |r| r.get(0),
        )?;
        Ok(DeleteImpact {
            subjects: 1,
            grades: grades as usize,
            attendance: attendance as usize,
            tasks: tasks as usize,
        })
    }

    pub fn profile_delete_impact(&self, profile_id: Uuid) -> StorageResult<DeleteImpact> {
        let conn = self.lock_conn()?;
        let pid = profile_id.to_string();
        let subjects: i64 = conn.query_row(
            "SELECT COUNT(*) FROM subjects WHERE profile_id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        let grades: i64 = conn.query_row(
            "SELECT COUNT(*) FROM grades WHERE profile_id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        let attendance: i64 = conn.query_row(
            "SELECT COUNT(*) FROM attendance WHERE profile_id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        let tasks: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        Ok(DeleteImpact {
            subjects: subjects as usize,
            grades: grades as usize,
            attendance: attendance as usize,
            tasks: tasks as usize,
        })
    }

    // ========================================================================
    // Global Search (FTS5 + Cyrillic-safe Unicode substring matching)
    // ========================================================================

    pub fn search_global(
        &self,
        query: &str,
        filter: &SearchFilter,
    ) -> StorageResult<Vec<SearchResultItem>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let limit = if filter.limit == 0 { 50 } else { filter.limit.min(200) };

        // Construct safe FTS5 prefix query from alphanumeric/Unicode tokens
        let tokens: Vec<String> = trimmed
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{}\"*", t.replace('"', "")))
            .collect();

        let conn = self.lock_conn()?;
        let mut results = Vec::new();

        if !tokens.is_empty() {
            let fts_expr = tokens.join(" AND ");
            let mut sql = String::from(
                "SELECT entity_type, entity_id, profile_id, subject_id, entity_date, title, body
                 FROM search_index WHERE search_index MATCH ?1",
            );
            let mut values: Vec<rusqlite::types::Value> = vec![fts_expr.into()];
            let mut idx = 2;

            if let Some(pid) = filter.profile_id {
                sql.push_str(&format!(" AND profile_id = ?{idx}"));
                values.push(pid.to_string().into());
                idx += 1;
            }
            if let Some(sid) = filter.subject_id {
                sql.push_str(&format!(" AND subject_id = ?{idx}"));
                values.push(sid.to_string().into());
                idx += 1;
            }
            if let Some(from) = filter.from_date {
                sql.push_str(&format!(" AND (entity_date = '' OR entity_date >= ?{idx})"));
                values.push(from.format("%Y-%m-%d").to_string().into());
                idx += 1;
            }
            if let Some(to) = filter.to_date {
                sql.push_str(&format!(" AND (entity_date = '' OR entity_date <= ?{idx})"));
                values.push(to.format("%Y-%m-%d").to_string().into());
                idx += 1;
            }
            sql.push_str(&format!(" ORDER BY rank LIMIT ?{idx}"));
            values.push(((limit * 2) as i64).into());

            if let Ok(mut stmt) = conn.prepare(&sql)
                && let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(values), |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                })
            {
                for r in rows.flatten() {
                    let (et, eid, pid, sid, ed, title, body) = r;
                    let Some(entity_type) = SearchEntityType::parse(&et) else {
                        continue;
                    };
                    if !filter.entity_types.is_empty() && !filter.entity_types.contains(&entity_type)
                    {
                        continue;
                    }
                    let Ok(entity_id) = parse_uuid(&eid) else {
                        continue;
                    };
                    let Ok(profile_id) = parse_uuid(&pid) else {
                        continue;
                    };
                    results.push(SearchResultItem {
                        entity_type,
                        entity_id,
                        profile_id,
                        subject_id: parse_opt_uuid(if sid.is_empty() { None } else { Some(sid) })?,
                        date: parse_opt_date(if ed.is_empty() { None } else { Some(ed) })?,
                        title,
                        snippet: body.chars().take(160).collect(),
                    });
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }

        // Cyrillic/substring fallback if FTS5 prefix returned nothing (e.g., mid-word match)
        if results.is_empty() {
            let needle = normalize_unicode(trimmed);
            let mut sql = String::from(
                "SELECT entity_type, entity_id, profile_id, subject_id, entity_date, title, body
                 FROM search_index WHERE 1=1",
            );
            let mut values: Vec<rusqlite::types::Value> = Vec::new();
            let mut idx = 1;
            if let Some(pid) = filter.profile_id {
                sql.push_str(&format!(" AND profile_id = ?{idx}"));
                values.push(pid.to_string().into());
                idx += 1;
            }
            if let Some(sid) = filter.subject_id {
                sql.push_str(&format!(" AND subject_id = ?{idx}"));
                values.push(sid.to_string().into());
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(values), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?;
            for r in rows {
                let (et, eid, pid, sid, ed, title, body) = r?;
                let Some(entity_type) = SearchEntityType::parse(&et) else {
                    continue;
                };
                if !filter.entity_types.is_empty() && !filter.entity_types.contains(&entity_type) {
                    continue;
                }
                if !normalize_unicode(&title).contains(&needle)
                    && !normalize_unicode(&body).contains(&needle)
                {
                    continue;
                }
                let date = parse_opt_date(if ed.is_empty() { None } else { Some(ed) })?;
                if let (Some(from), Some(d)) = (filter.from_date, date)
                    && d < from
                {
                    continue;
                }
                if let (Some(to), Some(d)) = (filter.to_date, date)
                    && d > to
                {
                    continue;
                }
                results.push(SearchResultItem {
                    entity_type,
                    entity_id: parse_uuid(&eid)?,
                    profile_id: parse_uuid(&pid)?,
                    subject_id: parse_opt_uuid(if sid.is_empty() { None } else { Some(sid) })?,
                    date,
                    title,
                    snippet: body.chars().take(160).collect(),
                });
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }

    // ========================================================================
    // Notifications & Reminders
    // ========================================================================

    pub fn insert_notification(&self, n: &NotificationItem) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO notifications (
                id, profile_id, kind, title, body, entity_type, entity_id,
                is_read, is_dismissed, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                n.id.to_string(),
                n.profile_id.map(|u| u.to_string()),
                n.kind.as_str(),
                n.title,
                n.body,
                n.entity_type,
                n.entity_id.map(|u| u.to_string()),
                if n.is_read { 1 } else { 0 },
                if n.is_dismissed { 1 } else { 0 },
                n.created_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, RecordSource::Gui)?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_notifications(
        &self,
        include_dismissed: bool,
        limit: usize,
    ) -> StorageResult<Vec<NotificationItem>> {
        let conn = self.lock_conn()?;
        let sql = if include_dismissed {
            "SELECT id, profile_id, kind, title, body, entity_type, entity_id, is_read, is_dismissed, created_at
             FROM notifications ORDER BY created_at DESC LIMIT ?1"
        } else {
            "SELECT id, profile_id, kind, title, body, entity_type, entity_id, is_read, is_dismissed, created_at
             FROM notifications WHERE is_dismissed = 0 ORDER BY created_at DESC LIMIT ?1"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, String>(9)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, kind, title, body, et, eid, read, dism, ca) = r?;
            out.push(NotificationItem {
                id: parse_uuid(&id)?,
                profile_id: parse_opt_uuid(pid)?,
                kind: NotificationKind::parse(&kind),
                title,
                body,
                entity_type: et,
                entity_id: parse_opt_uuid(eid)?,
                is_read: read != 0,
                is_dismissed: dism != 0,
                created_at: parse_dt(&ca)?,
            });
        }
        Ok(out)
    }

    pub fn mark_notification_read(&self, id: Uuid) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE notifications SET is_read = 1 WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn dismiss_notification(&self, id: Uuid) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE notifications SET is_dismissed = 1, is_read = 1 WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn mark_all_notifications_read(&self) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute("UPDATE notifications SET is_read = 1 WHERE is_read = 0", [])?;
        Ok(())
    }

    pub fn insert_reminder(&self, rem: &Reminder) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO reminders (id, task_id, remind_at, offset_minutes, is_fired, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rem.id.to_string(),
                rem.task_id.to_string(),
                rem.remind_at.to_rfc3339(),
                rem.offset_minutes,
                if rem.is_fired { 1 } else { 0 },
                rem.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_due_reminders(&self, now: DateTime<Utc>) -> StorageResult<Vec<Reminder>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, remind_at, offset_minutes, is_fired, created_at
             FROM reminders WHERE is_fired = 0 AND remind_at <= ?1 ORDER BY remind_at ASC",
        )?;
        let rows = stmt.query_map(params![now.to_rfc3339()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, tid, ra, off, fired, ca) = r?;
            out.push(Reminder {
                id: parse_uuid(&id)?,
                task_id: parse_uuid(&tid)?,
                remind_at: parse_dt(&ra)?,
                offset_minutes: off,
                is_fired: fired != 0,
                created_at: parse_dt(&ca)?,
            });
        }
        Ok(out)
    }

    pub fn mark_reminder_fired(&self, id: Uuid) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE reminders SET is_fired = 1 WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    pub fn list_task_reminders(&self, task_id: Uuid) -> StorageResult<Vec<Reminder>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, remind_at, offset_minutes, is_fired, created_at
             FROM reminders WHERE task_id = ?1 ORDER BY remind_at ASC",
        )?;
        let rows = stmt.query_map(params![task_id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, tid, ra, off, fired, ca) = r?;
            out.push(Reminder {
                id: parse_uuid(&id)?,
                task_id: parse_uuid(&tid)?,
                remind_at: parse_dt(&ra)?,
                offset_minutes: off,
                is_fired: fired != 0,
                created_at: parse_dt(&ca)?,
            });
        }
        Ok(out)
    }

    pub fn delete_task_reminders(&self, task_id: Uuid) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "DELETE FROM reminders WHERE task_id = ?1",
            params![task_id.to_string()],
        )?;
        Ok(())
    }

    // ========================================================================
    // Activity Log
    // ========================================================================

    pub fn insert_activity_log(&self, entry: &ActivityLogEntry) -> StorageResult<()> {
        let undo_json = entry
            .undo_payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO activity_log (
                id, timestamp, client_identity, tool_name, profile_id, entity_type,
                entity_id, operation_type, summary, result_status, source,
                correlation_id, undo_payload_json, undone
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                entry.id.to_string(),
                entry.timestamp.to_rfc3339(),
                entry.client_identity,
                entry.tool_name,
                entry.profile_id.map(|u| u.to_string()),
                entry.entity_type,
                entry.entity_id.map(|u| u.to_string()),
                entry.operation_type,
                entry.summary,
                entry.result_status,
                entry.source.as_str(),
                entry.correlation_id,
                undo_json,
                if entry.undone { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn list_activity_log(
        &self,
        profile_id: Option<Uuid>,
        source: Option<RecordSource>,
        limit: usize,
    ) -> StorageResult<Vec<ActivityLogEntry>> {
        let conn = self.lock_conn()?;
        let mut sql = String::from(
            "SELECT id, timestamp, client_identity, tool_name, profile_id, entity_type,
                    entity_id, operation_type, summary, result_status, source,
                    correlation_id, undo_payload_json, undone
             FROM activity_log WHERE 1=1",
        );
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        let mut idx = 1;
        if let Some(pid) = profile_id {
            sql.push_str(&format!(" AND profile_id = ?{idx}"));
            values.push(pid.to_string().into());
            idx += 1;
        }
        if let Some(src) = source {
            sql.push_str(&format!(" AND source = ?{idx}"));
            values.push(src.as_str().to_string().into());
            idx += 1;
        }
        sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ?{idx}"));
        values.push((limit as i64).into());

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, Option<String>>(12)?,
                r.get::<_, i64>(13)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, ts, cli, tool, pid, et, eid, op, sum, res, src, corr, undo, undone) = r?;
            let undo_payload = match undo {
                Some(s) if !s.is_empty() => serde_json::from_str(&s).ok(),
                _ => None,
            };
            out.push(ActivityLogEntry {
                id: parse_uuid(&id)?,
                timestamp: parse_dt(&ts)?,
                client_identity: cli,
                tool_name: tool,
                profile_id: parse_opt_uuid(pid)?,
                entity_type: et,
                entity_id: parse_opt_uuid(eid)?,
                operation_type: op,
                summary: sum,
                result_status: res,
                source: RecordSource::parse(&src).unwrap_or(RecordSource::Gui),
                correlation_id: corr,
                undo_payload,
                undone: undone != 0,
            });
        }
        Ok(out)
    }

    pub fn get_activity_log_entry(&self, id: Uuid) -> StorageResult<Option<ActivityLogEntry>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, timestamp, client_identity, tool_name, profile_id, entity_type,
                        entity_id, operation_type, summary, result_status, source,
                        correlation_id, undo_payload_json, undone
                 FROM activity_log WHERE id = ?1",
                params![id.to_string()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, String>(8)?,
                        r.get::<_, String>(9)?,
                        r.get::<_, String>(10)?,
                        r.get::<_, String>(11)?,
                        r.get::<_, Option<String>>(12)?,
                        r.get::<_, i64>(13)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, ts, cli, tool, pid, et, eid, op, sum, res, src, corr, undo, undone)) => {
                let undo_payload = match undo {
                    Some(s) if !s.is_empty() => serde_json::from_str(&s).ok(),
                    _ => None,
                };
                Ok(Some(ActivityLogEntry {
                    id: parse_uuid(&id)?,
                    timestamp: parse_dt(&ts)?,
                    client_identity: cli,
                    tool_name: tool,
                    profile_id: parse_opt_uuid(pid)?,
                    entity_type: et,
                    entity_id: parse_opt_uuid(eid)?,
                    operation_type: op,
                    summary: sum,
                    result_status: res,
                    source: RecordSource::parse(&src).unwrap_or(RecordSource::Gui),
                    correlation_id: corr,
                    undo_payload,
                    undone: undone != 0,
                }))
            }
        }
    }

    pub fn mark_activity_undone(&self, id: Uuid) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE activity_log SET undone = 1 WHERE id = ?1",
            params![id.to_string()],
        )?;
        Ok(())
    }

    // ========================================================================
    // MCP Idempotency Keys
    // ========================================================================

    pub fn get_idempotency(&self, key: &str) -> StorageResult<Option<IdempotencyRecord>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT idempotency_key, tool_name, request_hash, response_json
                 FROM mcp_idempotency_keys WHERE idempotency_key = ?1",
                params![key],
                |r| {
                    Ok(IdempotencyRecord {
                        key: r.get(0)?,
                        tool_name: r.get(1)?,
                        request_hash: r.get(2)?,
                        response_json: r.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn save_idempotency(&self, rec: &IdempotencyRecord) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO mcp_idempotency_keys (idempotency_key, tool_name, request_hash, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                rec.key,
                rec.tool_name,
                rec.request_hash,
                rec.response_json,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }
}
