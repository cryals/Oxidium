use chrono::NaiveDate;
use journal_domain::{
    AttendanceEntry, AttendanceStatus, DomainError, GradeEntry, MarkCategory, MarkType,
    RecordSource, SearchEntityType, Subject, WorkType,
};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::db::Database;
use crate::error::{StorageError, StorageResult};
use crate::helpers::{
    bump_sync_revision, delete_search_entry, parse_date, parse_decimal, parse_dt, parse_opt_decimal,
    parse_opt_time, parse_opt_uuid, parse_uuid, upsert_search_entry,
};

impl Database {
    // ========================================================================
    // Subjects
    // ========================================================================

    pub fn list_subjects(
        &self,
        profile_id: Uuid,
        include_archived: bool,
    ) -> StorageResult<Vec<Subject>> {
        let conn = self.lock_conn()?;
        let sql = if include_archived {
            "SELECT id, profile_id, academic_period_id, name, short_name, teacher, color, icon,
                    description, sort_order, is_archived, target_average, created_at, updated_at
             FROM subjects WHERE profile_id = ?1 ORDER BY is_archived ASC, sort_order ASC, name ASC"
        } else {
            "SELECT id, profile_id, academic_period_id, name, short_name, teacher, color, icon,
                    description, sort_order, is_archived, target_average, created_at, updated_at
             FROM subjects WHERE profile_id = ?1 AND is_archived = 0 ORDER BY sort_order ASC, name ASC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![profile_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i32>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, pid, ap, name, short, teacher, color, icon, desc, ord, arch, tgt, ca, ua) = r?;
            out.push(Subject {
                id: parse_uuid(&id)?,
                profile_id: parse_uuid(&pid)?,
                academic_period_id: parse_opt_uuid(ap)?,
                name,
                short_name: short,
                teacher,
                color,
                icon,
                description: desc,
                sort_order: ord,
                is_archived: arch != 0,
                target_average: parse_opt_decimal(tgt)?,
                created_at: parse_dt(&ca)?,
                updated_at: parse_dt(&ua)?,
            });
        }
        Ok(out)
    }

    pub fn get_subject(&self, id: Uuid) -> StorageResult<Option<Subject>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, academic_period_id, name, short_name, teacher, color, icon,
                        description, sort_order, is_archived, target_average, created_at, updated_at
                 FROM subjects WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, i32>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, pid, ap, name, short, teacher, color, icon, desc, ord, arch, tgt, ca, ua)) => {
                Ok(Some(Subject {
                    id: parse_uuid(&id)?,
                    profile_id: parse_uuid(&pid)?,
                    academic_period_id: parse_opt_uuid(ap)?,
                    name,
                    short_name: short,
                    teacher,
                    color,
                    icon,
                    description: desc,
                    sort_order: ord,
                    is_archived: arch != 0,
                    target_average: parse_opt_decimal(tgt)?,
                    created_at: parse_dt(&ca)?,
                    updated_at: parse_dt(&ua)?,
                }))
            }
        }
    }

    pub fn insert_subject(&self, subject: &Subject, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO subjects (
                id, profile_id, academic_period_id, name, short_name, teacher, color, icon,
                description, sort_order, is_archived, target_average, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                subject.id.to_string(),
                subject.profile_id.to_string(),
                subject.academic_period_id.map(|u| u.to_string()),
                subject.name,
                subject.short_name,
                subject.teacher,
                subject.color,
                subject.icon,
                subject.description,
                subject.sort_order,
                if subject.is_archived { 1 } else { 0 },
                subject.target_average.map(|d| d.to_string()),
                subject.created_at.to_rfc3339(),
                subject.updated_at.to_rfc3339(),
            ],
        )?;
        upsert_search_entry(
            &tx,
            SearchEntityType::Subject,
            subject.id,
            subject.profile_id,
            Some(subject.id),
            None,
            &subject.name,
            &format!("{} {} {}", subject.short_name, subject.teacher, subject.description),
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_subject(&self, subject: &Subject, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE subjects SET
                academic_period_id = ?2, name = ?3, short_name = ?4, teacher = ?5,
                color = ?6, icon = ?7, description = ?8, sort_order = ?9,
                is_archived = ?10, target_average = ?11, updated_at = ?12
             WHERE id = ?1",
            params![
                subject.id.to_string(),
                subject.academic_period_id.map(|u| u.to_string()),
                subject.name,
                subject.short_name,
                subject.teacher,
                subject.color,
                subject.icon,
                subject.description,
                subject.sort_order,
                if subject.is_archived { 1 } else { 0 },
                subject.target_average.map(|d| d.to_string()),
                subject.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "subject".into(),
                id: subject.id.to_string(),
            }));
        }
        upsert_search_entry(
            &tx,
            SearchEntityType::Subject,
            subject.id,
            subject.profile_id,
            Some(subject.id),
            None,
            &subject.name,
            &format!("{} {} {}", subject.short_name, subject.teacher, subject.description),
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn reorder_subjects(
        &self,
        profile_id: Uuid,
        ordered_ids: &[Uuid],
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        for (idx, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE subjects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND profile_id = ?4",
                params![idx as i32, now, id.to_string(), profile_id.to_string()],
            )?;
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_subject_explicit(
        &self,
        subject_id: Uuid,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let sid = subject_id.to_string();
        tx.execute("DELETE FROM search_index WHERE subject_id = ?1", params![sid])?;
        tx.execute("DELETE FROM grades WHERE subject_id = ?1", params![sid])?;
        tx.execute("DELETE FROM attendance WHERE subject_id = ?1", params![sid])?;
        tx.execute("UPDATE tasks SET subject_id = NULL WHERE subject_id = ?1", params![sid])?;
        let changed = tx.execute("DELETE FROM subjects WHERE id = ?1", params![sid])?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "subject".into(),
                id: sid,
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Mark Types
    // ========================================================================

    pub fn list_mark_types(&self, profile_id: Option<Uuid>) -> StorageResult<Vec<MarkType>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, code, name, short_label, category, color, icon,
                    counts_in_average, numeric_value, is_builtin, sort_order
             FROM mark_types
             WHERE profile_id IS NULL OR profile_id = ?1
             ORDER BY is_builtin DESC, sort_order ASC, name ASC",
        )?;
        let pid = profile_id.map(|u| u.to_string()).unwrap_or_default();
        let rows = stmt.query_map(params![pid], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i32>(11)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, p, code, name, short, cat, color, icon, counts, num, builtin, ord) = r?;
            out.push(MarkType {
                id: parse_uuid(&id)?,
                profile_id: parse_opt_uuid(p)?,
                code,
                name,
                short_label: short,
                category: MarkCategory::parse(&cat).unwrap_or(MarkCategory::CustomStatus),
                color,
                icon,
                counts_in_average: counts != 0,
                numeric_value: parse_opt_decimal(num)?,
                is_builtin: builtin != 0,
                sort_order: ord,
            });
        }
        Ok(out)
    }

    pub fn get_mark_type(&self, id: Uuid) -> StorageResult<Option<MarkType>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, code, name, short_label, category, color, icon,
                        counts_in_average, numeric_value, is_builtin, sort_order
                 FROM mark_types WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i32>(11)?,
                    ))
                },
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((id, p, code, name, short, cat, color, icon, counts, num, builtin, ord)) => {
                Ok(Some(MarkType {
                    id: parse_uuid(&id)?,
                    profile_id: parse_opt_uuid(p)?,
                    code,
                    name,
                    short_label: short,
                    category: MarkCategory::parse(&cat).unwrap_or(MarkCategory::CustomStatus),
                    color,
                    icon,
                    counts_in_average: counts != 0,
                    numeric_value: parse_opt_decimal(num)?,
                    is_builtin: builtin != 0,
                    sort_order: ord,
                }))
            }
        }
    }

    pub fn insert_mark_type(&self, mt: &MarkType, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO mark_types (
                id, profile_id, code, name, short_label, category, color, icon,
                counts_in_average, numeric_value, is_builtin, sort_order
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                mt.id.to_string(),
                mt.profile_id.map(|u| u.to_string()),
                mt.code,
                mt.name,
                mt.short_label,
                mt.category.as_str(),
                mt.color,
                mt.icon,
                if mt.counts_in_average { 1 } else { 0 },
                mt.numeric_value.map(|d| d.to_string()),
                if mt.is_builtin { 1 } else { 0 },
                mt.sort_order,
            ],
        )?;
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_custom_mark_type(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "DELETE FROM mark_types WHERE id = ?1 AND is_builtin = 0",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::InvalidState(
                "cannot delete built-in or non-existent mark type".into(),
            )));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Grades
    // ========================================================================

    pub fn insert_grade(&self, grade: &GradeEntry) -> StorageResult<()> {
        self.insert_grades_batch(std::slice::from_ref(grade), grade.source)
    }

    /// Atomically inserts a batch of grades inside a single SQLite transaction.
    pub fn insert_grades_batch(
        &self,
        grades: &[GradeEntry],
        source: RecordSource,
    ) -> StorageResult<()> {
        if grades.is_empty() {
            return Ok(());
        }
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO grades (
                    id, profile_id, subject_id, date, time, mark_type_id, numeric_value,
                    weight, work_type, topic, comment, teacher, source, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;
            for g in grades {
                stmt.execute(params![
                    g.id.to_string(),
                    g.profile_id.to_string(),
                    g.subject_id.to_string(),
                    g.date.format("%Y-%m-%d").to_string(),
                    g.time.map(|t| t.format("%H:%M:%S").to_string()),
                    g.mark_type_id.to_string(),
                    g.numeric_value.map(|d| d.to_string()),
                    g.weight.to_string(),
                    g.work_type.as_str(),
                    g.topic,
                    g.comment,
                    g.teacher,
                    g.source.as_str(),
                    g.created_at.to_rfc3339(),
                    g.updated_at.to_rfc3339(),
                ])?;
                if !g.topic.is_empty() || !g.comment.is_empty() {
                    upsert_search_entry(
                        &tx,
                        SearchEntityType::Grade,
                        g.id,
                        g.profile_id,
                        Some(g.subject_id),
                        Some(g.date),
                        if g.topic.is_empty() { &g.mark_label } else { &g.topic },
                        &format!("{} {} {}", g.mark_label, g.topic, g.comment),
                    )?;
                }
            }
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_grade(&self, grade: &GradeEntry, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE grades SET
                subject_id = ?2, date = ?3, time = ?4, mark_type_id = ?5,
                numeric_value = ?6, weight = ?7, work_type = ?8, topic = ?9,
                comment = ?10, teacher = ?11, updated_at = ?12
             WHERE id = ?1",
            params![
                grade.id.to_string(),
                grade.subject_id.to_string(),
                grade.date.format("%Y-%m-%d").to_string(),
                grade.time.map(|t| t.format("%H:%M:%S").to_string()),
                grade.mark_type_id.to_string(),
                grade.numeric_value.map(|d| d.to_string()),
                grade.weight.to_string(),
                grade.work_type.as_str(),
                grade.topic,
                grade.comment,
                grade.teacher,
                grade.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "grade".into(),
                id: grade.id.to_string(),
            }));
        }
        if !grade.topic.is_empty() || !grade.comment.is_empty() {
            upsert_search_entry(
                &tx,
                SearchEntityType::Grade,
                grade.id,
                grade.profile_id,
                Some(grade.subject_id),
                Some(grade.date),
                if grade.topic.is_empty() {
                    &grade.mark_label
                } else {
                    &grade.topic
                },
                &format!("{} {} {}", grade.mark_label, grade.topic, grade.comment),
            )?;
        } else {
            delete_search_entry(&tx, SearchEntityType::Grade, grade.id)?;
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_grade(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        delete_search_entry(&tx, SearchEntityType::Grade, id)?;
        let changed = tx.execute("DELETE FROM grades WHERE id = ?1", params![id.to_string()])?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "grade".into(),
                id: id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_grade(&self, id: Uuid) -> StorageResult<Option<GradeEntry>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT g.id, g.profile_id, g.subject_id, g.date, g.time, g.mark_type_id,
                        mt.code, mt.short_label, mt.category, mt.counts_in_average,
                        g.numeric_value, g.weight, g.work_type, g.topic, g.comment,
                        g.teacher, g.source, g.created_at, g.updated_at
                 FROM grades g
                 JOIN mark_types mt ON mt.id = g.mark_type_id
                 WHERE g.id = ?1",
                params![id.to_string()],
                Self::map_grade_row,
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some(raw) => Ok(Some(Self::decode_grade_row(raw)?)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn list_grades(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
        work_type: Option<WorkType>,
        mark_type_id: Option<Uuid>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> StorageResult<Vec<GradeEntry>> {
        let conn = self.lock_conn()?;
        let mut sql = String::from(
            "SELECT g.id, g.profile_id, g.subject_id, g.date, g.time, g.mark_type_id,
                    mt.code, mt.short_label, mt.category, mt.counts_in_average,
                    g.numeric_value, g.weight, g.work_type, g.topic, g.comment,
                    g.teacher, g.source, g.created_at, g.updated_at
             FROM grades g
             JOIN mark_types mt ON mt.id = g.mark_type_id
             WHERE g.profile_id = ?1",
        );
        let mut values: Vec<rusqlite::types::Value> = vec![profile_id.to_string().into()];
        let mut idx = 2;

        if let Some(sid) = subject_id {
            sql.push_str(&format!(" AND g.subject_id = ?{idx}"));
            values.push(sid.to_string().into());
            idx += 1;
        }
        if let Some(from) = from_date {
            sql.push_str(&format!(" AND g.date >= ?{idx}"));
            values.push(from.format("%Y-%m-%d").to_string().into());
            idx += 1;
        }
        if let Some(to) = to_date {
            sql.push_str(&format!(" AND g.date <= ?{idx}"));
            values.push(to.format("%Y-%m-%d").to_string().into());
            idx += 1;
        }
        if let Some(wt) = work_type {
            sql.push_str(&format!(" AND g.work_type = ?{idx}"));
            values.push(wt.as_str().to_string().into());
            idx += 1;
        }
        if let Some(mt) = mark_type_id {
            sql.push_str(&format!(" AND g.mark_type_id = ?{idx}"));
            values.push(mt.to_string().into());
            idx += 1;
        }

        sql.push_str(" ORDER BY g.date ASC, g.created_at ASC");

        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT ?{idx}"));
            values.push((lim as i64).into());
            idx += 1;
            if let Some(off) = offset {
                sql.push_str(&format!(" OFFSET ?{idx}"));
                values.push((off as i64).into());
            }
        }

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(values), Self::map_grade_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(Self::decode_grade_row(r?)?);
        }
        Ok(out)
    }

    #[allow(clippy::type_complexity)]
    fn map_grade_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
            row.get(12)?,
            row.get(13)?,
            row.get(14)?,
            row.get(15)?,
            row.get(16)?,
            row.get(17)?,
            row.get(18)?,
        ))
    }

    #[allow(clippy::type_complexity)]
    fn decode_grade_row(
        raw: (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            i64,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    ) -> StorageResult<GradeEntry> {
        let (
            id,
            pid,
            sid,
            date,
            time,
            mt_id,
            mt_code,
            mt_label,
            mt_cat,
            counts,
            num_val,
            weight,
            wt,
            topic,
            comment,
            teacher,
            src,
            ca,
            ua,
        ) = raw;
        Ok(GradeEntry {
            id: parse_uuid(&id)?,
            profile_id: parse_uuid(&pid)?,
            subject_id: parse_uuid(&sid)?,
            date: parse_date(&date)?,
            time: parse_opt_time(time)?,
            mark_type_id: parse_uuid(&mt_id)?,
            mark_code: mt_code,
            mark_label: mt_label,
            mark_category: MarkCategory::parse(&mt_cat).unwrap_or(MarkCategory::CustomStatus),
            counts_in_average: counts != 0,
            numeric_value: parse_opt_decimal(num_val)?,
            weight: parse_decimal(&weight)?,
            work_type: WorkType::parse(&wt).unwrap_or(WorkType::Other),
            topic,
            comment,
            teacher,
            source: RecordSource::parse(&src).unwrap_or(RecordSource::Gui),
            created_at: parse_dt(&ca)?,
            updated_at: parse_dt(&ua)?,
        })
    }

    // ========================================================================
    // Attendance (Stored independently from numeric grades)
    // ========================================================================

    pub fn insert_attendance(&self, entry: &AttendanceEntry) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO attendance (
                id, profile_id, subject_id, date, time, status, mark_type_id,
                reason, comment, source, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                entry.id.to_string(),
                entry.profile_id.to_string(),
                entry.subject_id.to_string(),
                entry.date.format("%Y-%m-%d").to_string(),
                entry.time.map(|t| t.format("%H:%M:%S").to_string()),
                entry.status.as_db_str(),
                entry.mark_type_id.to_string(),
                entry.reason,
                entry.comment,
                entry.source.as_str(),
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        if !entry.reason.is_empty() || !entry.comment.is_empty() {
            upsert_search_entry(
                &tx,
                SearchEntityType::Attendance,
                entry.id,
                entry.profile_id,
                Some(entry.subject_id),
                Some(entry.date),
                &entry.status.as_db_str(),
                &format!("{} {}", entry.reason, entry.comment),
            )?;
        }
        bump_sync_revision(&tx, entry.source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_attendance(
        &self,
        entry: &AttendanceEntry,
        source: RecordSource,
    ) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        let changed = tx.execute(
            "UPDATE attendance SET
                subject_id = ?2, date = ?3, time = ?4, status = ?5,
                mark_type_id = ?6, reason = ?7, comment = ?8, updated_at = ?9
             WHERE id = ?1",
            params![
                entry.id.to_string(),
                entry.subject_id.to_string(),
                entry.date.format("%Y-%m-%d").to_string(),
                entry.time.map(|t| t.format("%H:%M:%S").to_string()),
                entry.status.as_db_str(),
                entry.mark_type_id.to_string(),
                entry.reason,
                entry.comment,
                entry.updated_at.to_rfc3339(),
            ],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "attendance".into(),
                id: entry.id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_attendance(&self, id: Uuid, source: RecordSource) -> StorageResult<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        delete_search_entry(&tx, SearchEntityType::Attendance, id)?;
        let changed = tx.execute(
            "DELETE FROM attendance WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed == 0 {
            return Err(StorageError::Domain(DomainError::NotFound {
                entity: "attendance".into(),
                id: id.to_string(),
            }));
        }
        bump_sync_revision(&tx, source)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_attendance(&self, id: Uuid) -> StorageResult<Option<AttendanceEntry>> {
        let conn = self.lock_conn()?;
        let row = conn
            .query_row(
                "SELECT id, profile_id, subject_id, date, time, status, mark_type_id,
                        reason, comment, source, created_at, updated_at
                 FROM attendance WHERE id = ?1",
                params![id.to_string()],
                Self::map_attendance_row,
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(Self::decode_attendance_row(r)?)),
        }
    }

    pub fn list_attendance(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
    ) -> StorageResult<Vec<AttendanceEntry>> {
        let conn = self.lock_conn()?;
        let mut sql = String::from(
            "SELECT id, profile_id, subject_id, date, time, status, mark_type_id,
                    reason, comment, source, created_at, updated_at
             FROM attendance WHERE profile_id = ?1",
        );
        let mut values: Vec<rusqlite::types::Value> = vec![profile_id.to_string().into()];
        let mut idx = 2;
        if let Some(sid) = subject_id {
            sql.push_str(&format!(" AND subject_id = ?{idx}"));
            values.push(sid.to_string().into());
            idx += 1;
        }
        if let Some(from) = from_date {
            sql.push_str(&format!(" AND date >= ?{idx}"));
            values.push(from.format("%Y-%m-%d").to_string().into());
            idx += 1;
        }
        if let Some(to) = to_date {
            sql.push_str(&format!(" AND date <= ?{idx}"));
            values.push(to.format("%Y-%m-%d").to_string().into());
        }
        sql.push_str(" ORDER BY date ASC, created_at ASC");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(values),
            Self::map_attendance_row,
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(Self::decode_attendance_row(r?)?);
        }
        Ok(out)
    }

    #[allow(clippy::type_complexity)]
    fn map_attendance_row(
        row: &rusqlite::Row<'_>,
    ) -> rusqlite::Result<(
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
        ))
    }

    #[allow(clippy::type_complexity)]
    fn decode_attendance_row(
        raw: (
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    ) -> StorageResult<AttendanceEntry> {
        let (id, pid, sid, date, time, status, mt_id, reason, comment, src, ca, ua) = raw;
        Ok(AttendanceEntry {
            id: parse_uuid(&id)?,
            profile_id: parse_uuid(&pid)?,
            subject_id: parse_uuid(&sid)?,
            date: parse_date(&date)?,
            time: parse_opt_time(time)?,
            status: AttendanceStatus::parse(&status).unwrap_or(AttendanceStatus::Absent),
            mark_type_id: parse_uuid(&mt_id)?,
            reason,
            comment,
            source: RecordSource::parse(&src).unwrap_or(RecordSource::Gui),
            created_at: parse_dt(&ca)?,
            updated_at: parse_dt(&ua)?,
        })
    }
}
