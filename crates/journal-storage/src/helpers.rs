use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use journal_domain::{RecordSource, SearchEntityType};
use rusqlite::{Connection, params};
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::{StorageError, StorageResult};

pub fn parse_uuid(s: &str) -> StorageResult<Uuid> {
    Uuid::parse_str(s).map_err(|e| StorageError::DataParse(format!("invalid uuid '{s}': {e}")))
}

pub fn parse_opt_uuid(s: Option<String>) -> StorageResult<Option<Uuid>> {
    match s {
        Some(v) if !v.is_empty() => Ok(Some(parse_uuid(&v)?)),
        _ => Ok(None),
    }
}

pub fn parse_dt(s: &str) -> StorageResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StorageError::DataParse(format!("invalid timestamp '{s}': {e}")))
}

pub fn parse_opt_dt(s: Option<String>) -> StorageResult<Option<DateTime<Utc>>> {
    match s {
        Some(v) if !v.is_empty() => Ok(Some(parse_dt(&v)?)),
        _ => Ok(None),
    }
}

pub fn parse_date(s: &str) -> StorageResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| StorageError::DataParse(format!("invalid date '{s}': {e}")))
}

pub fn parse_opt_date(s: Option<String>) -> StorageResult<Option<NaiveDate>> {
    match s {
        Some(v) if !v.is_empty() => Ok(Some(parse_date(&v)?)),
        _ => Ok(None),
    }
}

pub fn parse_opt_time(s: Option<String>) -> StorageResult<Option<NaiveTime>> {
    match s {
        Some(v) if !v.is_empty() => NaiveTime::parse_from_str(&v, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(&v, "%H:%M"))
            .map(Some)
            .map_err(|e| StorageError::DataParse(format!("invalid time '{v}': {e}"))),
        _ => Ok(None),
    }
}

pub fn parse_decimal(s: &str) -> StorageResult<Decimal> {
    Decimal::from_str(s).map_err(|e| StorageError::DataParse(format!("invalid decimal '{s}': {e}")))
}

pub fn parse_opt_decimal(s: Option<String>) -> StorageResult<Option<Decimal>> {
    match s {
        Some(v) if !v.is_empty() => Ok(Some(parse_decimal(&v)?)),
        _ => Ok(None),
    }
}

pub fn normalize_unicode(s: &str) -> String {
    s.trim().to_lowercase()
}

pub fn bump_sync_revision(conn: &Connection, source: RecordSource) -> StorageResult<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sync_state SET revision = revision + 1, updated_at = ?1, last_source = ?2 WHERE id = 1",
        params![now, source.as_str()],
    )?;
    let rev: i64 = conn.query_row("SELECT revision FROM sync_state WHERE id = 1", [], |r| {
        r.get(0)
    })?;
    Ok(rev)
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_search_entry(
    conn: &Connection,
    entity_type: SearchEntityType,
    entity_id: Uuid,
    profile_id: Uuid,
    subject_id: Option<Uuid>,
    entity_date: Option<NaiveDate>,
    title: &str,
    body: &str,
) -> StorageResult<()> {
    let et = entity_type.as_str();
    let eid = entity_id.to_string();
    conn.execute(
        "DELETE FROM search_index WHERE entity_type = ?1 AND entity_id = ?2",
        params![et, eid],
    )?;
    conn.execute(
        "INSERT INTO search_index (entity_type, entity_id, profile_id, subject_id, entity_date, title, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            et,
            eid,
            profile_id.to_string(),
            subject_id.map(|id| id.to_string()).unwrap_or_default(),
            entity_date.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            title,
            body,
        ],
    )?;
    Ok(())
}

pub fn delete_search_entry(
    conn: &Connection,
    entity_type: SearchEntityType,
    entity_id: Uuid,
) -> StorageResult<()> {
    conn.execute(
        "DELETE FROM search_index WHERE entity_type = ?1 AND entity_id = ?2",
        params![entity_type.as_str(), entity_id.to_string()],
    )?;
    Ok(())
}
