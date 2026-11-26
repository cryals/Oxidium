use chrono::Utc;
use journal_domain::{
    AcademicPeriod, AppSettings, DomainError, Holiday, PeriodType, Profile, RecordSource,
};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::db::Database;
use crate::error::{StorageError, StorageResult};
use crate::helpers::{bump_sync_revision, parse_date, parse_dt, parse_opt_uuid, parse_uuid};

impl Database {
    // ========================================================================
    // Profiles
    // ========================================================================

    pub fn list_profiles(&self, include_archived: bool) -> StorageResult<Vec<Profile>> {
        let conn = self.lock_conn()?;
        let sql = if include_archived {
            "SELECT id, name, icon, color, institution, group_name, academic_year, description, is_archived, created_at, updated_at
             FROM profiles ORDER BY is_archived ASC, created_at ASC"
        } else {
            "SELECT id, name, icon, color, institution, group_name, academic_year, description, is_archived, created_at, updated_at
             FROM profiles WHERE is_archived = 0 ORDER BY created_at ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, name, icon, color, inst, grp, year, desc, arch, ca, ua) = r?;
            out.push(Profile {
                id: parse_uuid(&id)?,
                name,
                icon,
                color,
                institution: inst,
                group_name: grp,
                academic_year: year,
                description: desc,
                is_archived: arch != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            });
        }
        Ok(out)
    }

    pub fn get_profile(&self, id: Uuid) -> StorageResult<Option<Profile>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, name, icon, color, institution, group_name, academic_year, description, is_archived, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, name, icon, color, inst, grp, year, desc, arch, ca, ua)) => Ok(Some(Profile {
                id: parse_uuid(&id)?,
                name,
                icon,
                color,
                institution: inst,
                group_name: grp,
                academic_year: year,
                description: desc,
                is_archived: arch != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            })),
        }
    }

    pub fn insert_profile(&self, profile: &Profile, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO profiles (
                id, name, icon, color, institution, group_name, academic_year,
                description, is_archived, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                profile.id.to_string(),
                profile.name,
                profile.icon,
                profile.color,
                profile.institution,
                profile.group_name,
                profile.academic_year,
                profile.description,
                if profile.is_archived { 1 } else { 0 },
                profile.created_at.to_rfc3339(),
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_profile(&self, profile: &Profile, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE profiles SET
                name = ?2, icon = ?3, color = ?4, institution = ?5,
                group_name = ?6, academic_year = ?7, description = ?8,
                is_archived = ?9, updated_at = ?10
             WHERE id = ?1",
            params![
                profile.id.to_string(),
                profile.name,
                profile.icon,
                profile.color,
                profile.institution,
                profile.group_name,
                profile.academic_year,
                profile.description,
                if profile.is_archived { 1 } else { 0 },
                profile.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "profile".into(),
                id: profile.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    /// Explicitly deletes a profile and all its owned entities inside a single atomic transaction.
    pub fn delete_profile_cascade_explicit(
        &self,
        profile_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let pid = profile_id.to_string();
        tx.execute("DELETE FROM search_index WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM grades WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM attendance WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM tasks WHERE profile_id = ?1", params![pid])?;
        tx.execute(
            "DELETE FROM kanban_columns WHERE board_id IN (SELECT id FROM boards WHERE profile_id = ?1)",
            params![pid],
        )?;
        tx.execute("DELETE FROM boards WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM tags WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM badges WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM subjects WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM academic_periods WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM mark_types WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM holidays WHERE profile_id = ?1", params![pid])?;
        tx.execute("DELETE FROM notifications WHERE profile_id = ?1", params![pid])?;
        let changed = tx.execute("DELETE FROM profiles WHERE id = ?1", params![pid])?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "profile".into(),
                id: pid,
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Academic Periods
    // ========================================================================

    pub fn list_academic_periods(
        &self,
        profile_id: Uuid,
        include_archived: bool,
    ) -> StorageResult<Vec<AcademicPeriod>> {
        let conn = self.lock_conn()?;
        let sql = if include_archived {
            "SELECT id, profile_id, name, period_type, start_date, end_date, is_archived, created_at, updated_at
             FROM academic_periods WHERE profile_id = ?1 ORDER BY start_date DESC"
        } else {
            "SELECT id, profile_id, name, period_type, start_date, end_date, is_archived, created_at, updated_at
             FROM academic_periods WHERE profile_id = ?1 AND is_archived = 0 ORDER BY start_date DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![profile_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, name, pt, sd, ed, arch, ca, ua) = r?;
            out.push(AcademicPeriod {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                name,
                period_type: PeriodType::parse(&pt).unwrap_or(PeriodType::Custom),
                start_date: parse_date(&sd)?,
                end_date: parse_date(&ed)?,
                is_archived: arch != 0,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            });
        }
        Ok(out)
    }

    pub fn insert_academic_period(
        &self,
        period: &AcademicPeriod,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO academic_periods (
                id, profile_id, name, period_type, start_date, end_date, is_archived, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                period.id.to_string(),
                period.profile_id.to_string(),
                period.name,
                period.period_type.as_str(),
                period.start_date.format("%Y-%m-%d").to_string(),
                period.end_date.format("%Y-%m-%d").to_string(),
                if period.is_archived { 1 } else { 0 },
                period.created_at.to_rfc3339(),
                period.updated_at.to_rfc3339(),
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_academic_period(
        &self,
        period: &AcademicPeriod,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE academic_periods SET
                name = ?2, period_type = ?3, start_date = ?4, end_date = ?5,
                is_archived = ?6, updated_at = ?7
             WHERE id = ?1",
            params![
                period.id.to_string(),
                period.name,
                period.period_type.as_str(),
                period.start_date.format("%Y-%m-%d").to_string(),
                period.end_date.format("%Y-%m-%d").to_string(),
                if period.is_archived { 1 } else { 0 },
                period.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "academic_period".into(),
                id: period.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_academic_period(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM academic_periods WHERE id = ?1", params![id.to_string()])?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Holidays
    // ========================================================================

    pub fn list_holidays(&self, profile_id: Option<Uuid>) -> StorageResult<Vec<Holiday>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, date, name FROM holidays
             WHERE profile_id IS NULL OR profile_id = ?1
             ORDER BY date ASC",
        )?;
        let pid = profile_id.map(|u| u.to_string()).unwrap_or_default();
        let rows = stmt.query_map(params![pid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, p, d, name) = r?;
            out.push(Holiday {
                id: parse_uuid(&id)?,
                profile_id: parse_opt_uuid(p)?,
                date: parse_date(&d)?,
                name,
            });
        }
        Ok(out)
    }

    pub fn insert_holiday(&self, holiday: &Holiday, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO holidays (id, profile_id, date, name) VALUES (?1, ?2, ?3, ?4)",
            params![
                holiday.id.to_string(),
                holiday.profile_id.map(|u| u.to_string()),
                holiday.date.format("%Y-%m-%d").to_string(),
                holiday.name,
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_holiday(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM holidays WHERE id = ?1", params![id.to_string()])?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Settings
    // ========================================================================

    pub fn load_settings(&self) -> StorageResult<AppSettings> {
        let conn = self.lock_conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = 'app_settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match raw {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&self, settings: &AppSettings, source: RecordSource) -> StorageResult<()> {
        let json = serde_json::to_string(settings)?;
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES ('app_settings', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![json, Utc::now().to_rfc3339()],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_setting_raw(&self, key: &str) -> StorageResult<Option<String>> {
        let conn = self.lock_conn()?;
        let raw: Option<String> = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(raw)
    }

    pub fn set_setting_raw(&self, key: &str, value_json: &str) -> StorageResult<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![key, value_json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
}
