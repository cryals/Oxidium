use chrono::{NaiveDate, NaiveTime, Utc};
use journal_domain::{
    AttendanceEntry, AttendanceStatus, AverageMode, CalendarDay, DomainError, GradeEntry,
    MarkCategory, MarkType, RecordSource, Subject, WorkType, build_date_days,
    build_month_calendar, builtin_marks, calculate_average, format_average, month_bounds,
    validate_batch_size, validate_optional_text, validate_weight, week_bounds, MAX_TEXT_LEN,
    MAX_TITLE_LEN,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::AppResult;
use crate::services::AppServices;
use crate::services::ops::AuditMeta;
use crate::undo::UndoCommand;

#[derive(Debug, Clone, Deserialize)]
pub struct AddGradeInput {
    pub profile_id: Uuid,
    pub subject_id: Uuid,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub mark_type_id: Option<Uuid>,
    pub mark_code: Option<String>,
    pub numeric_grade: Option<u8>,
    pub weight: Option<Decimal>,
    pub work_type: Option<WorkType>,
    pub topic: Option<String>,
    pub comment: Option<String>,
    pub teacher: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateGradeInput {
    pub subject_id: Option<Uuid>,
    pub date: Option<NaiveDate>,
    pub time: Option<Option<NaiveTime>>,
    pub mark_type_id: Option<Uuid>,
    pub weight: Option<Decimal>,
    pub work_type: Option<WorkType>,
    pub topic: Option<String>,
    pub comment: Option<String>,
    pub teacher: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddAttendanceInput {
    pub profile_id: Uuid,
    pub subject_id: Uuid,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub status: AttendanceStatus,
    pub reason: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateAttendanceInput {
    pub date: Option<NaiveDate>,
    pub time: Option<Option<NaiveTime>>,
    pub status: Option<AttendanceStatus>,
    pub reason: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalCell {
    pub subject_id: Uuid,
    pub date: NaiveDate,
    pub grades: Vec<GradeEntry>,
    pub attendance: Vec<AttendanceEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalSubjectRow {
    pub subject: Subject,
    pub cells_by_date: HashMap<NaiveDate, JournalCell>,
    pub average: Option<Decimal>,
    pub average_formatted: String,
    pub numeric_grade_count: usize,
    pub absence_count: usize,
    pub excused_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct JournalMatrix {
    pub profile_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub days: Vec<CalendarDay>,
    pub rows: Vec<JournalSubjectRow>,
    pub overall_average: Option<Decimal>,
    pub overall_average_formatted: String,
    pub average_mode: AverageMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodSummary {
    pub profile_id: Uuid,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub total_grades: usize,
    pub numeric_grades: usize,
    pub average: Option<Decimal>,
    pub average_formatted: String,
    pub absences: usize,
    pub excused_absences: usize,
    pub present_records: usize,
    pub grades_by_subject: Vec<SubjectSummaryItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectSummaryItem {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub grade_count: usize,
    pub average: Option<Decimal>,
    pub average_formatted: String,
    pub absences: usize,
    pub excused: usize,
}

impl AppServices {
    fn resolve_mark_type(&self, profile_id: Uuid, input: &AddGradeInput) -> AppResult<MarkType> {
        if let Some(mt_id) = input.mark_type_id
            && let Some(mt) = self.db.get_mark_type(mt_id)?
        {
            return Ok(mt);
        }
        if let Some(num) = input.numeric_grade
            && let Some(id) = builtin_marks::grade_id_for_value(num)
            && let Some(mt) = self.db.get_mark_type(id)?
        {
            return Ok(mt);
        }
        if let Some(ref code) = input.mark_code {
            let needle = code.trim().to_lowercase();
            let all = self.db.list_mark_types(Some(profile_id))?;
            if let Some(mt) = all.into_iter().find(|m| {
                m.code.to_lowercase() == needle || m.short_label.to_lowercase() == needle
            }) {
                return Ok(mt);
            }
        }
        Err(DomainError::validation(
            "mark_type",
            "provide a valid mark_type_id, mark_code, or numeric_grade (1-5)",
        )
        .into())
    }

    fn verify_subject_in_profile(&self, profile_id: Uuid, subject_id: Uuid) -> AppResult<Subject> {
        let subject = self.get_subject(subject_id)?;
        if subject.profile_id != profile_id {
            return Err(DomainError::ProfileIsolation(format!(
                "subject {subject_id} does not belong to profile {profile_id}"
            ))
            .into());
        }
        Ok(subject)
    }

    pub fn add_grade(
        &self,
        input: AddGradeInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<GradeEntry> {
        let subject = self.verify_subject_in_profile(input.profile_id, input.subject_id)?;
        let mt = self.resolve_mark_type(input.profile_id, &input)?;
        if mt.category == MarkCategory::Attendance {
            return Err(DomainError::validation(
                "mark_type",
                "attendance marks must be recorded via attendance service, not numeric grades",
            )
            .into());
        }
        let work_type = input.work_type.unwrap_or(WorkType::ClassAnswer);
        let weight = validate_weight(input.weight.unwrap_or_else(|| work_type.default_weight()))?;
        let now = Utc::now();
        let entry = GradeEntry {
            id: Uuid::new_v4(),
            profile_id: input.profile_id,
            subject_id: subject.id,
            date: input.date,
            time: input.time,
            mark_type_id: mt.id,
            mark_code: mt.code,
            mark_label: mt.short_label.clone(),
            mark_category: mt.category,
            counts_in_average: mt.counts_in_average,
            numeric_value: mt.numeric_value,
            weight,
            work_type,
            topic: validate_optional_text("topic", input.topic.as_deref().unwrap_or(""), MAX_TITLE_LEN)?,
            comment: validate_optional_text(
                "comment",
                input.comment.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?,
            teacher: validate_optional_text(
                "teacher",
                input.teacher.as_deref().unwrap_or(&subject.teacher),
                MAX_TITLE_LEN,
            )?,
            source,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_grade(&entry)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::GradeAdded {
            grade: entry.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(entry.profile_id),
            "grade",
            Some(entry.id),
            "create",
            format!("Добавлена отметка «{}» по предмету «{}» ({})", entry.mark_label, subject.name, entry.date),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "grade");
        Ok(entry)
    }

    /// Atomically validates and inserts a batch of grades inside a single transaction.
    pub fn add_grades_batch(
        &self,
        inputs: Vec<AddGradeInput>,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<Vec<GradeEntry>> {
        validate_batch_size(inputs.len())?;
        let mut entries = Vec::with_capacity(inputs.len());
        let now = Utc::now();
        for input in inputs {
            let subject = self.verify_subject_in_profile(input.profile_id, input.subject_id)?;
            let mt = self.resolve_mark_type(input.profile_id, &input)?;
            if mt.category == MarkCategory::Attendance {
                return Err(DomainError::validation(
                    "mark_type",
                    "attendance marks cannot be inserted in grades batch",
                )
                .into());
            }
            let work_type = input.work_type.unwrap_or(WorkType::ClassAnswer);
            let weight = validate_weight(input.weight.unwrap_or_else(|| work_type.default_weight()))?;
            entries.push(GradeEntry {
                id: Uuid::new_v4(),
                profile_id: input.profile_id,
                subject_id: subject.id,
                date: input.date,
                time: input.time,
                mark_type_id: mt.id,
                mark_code: mt.code,
                mark_label: mt.short_label,
                mark_category: mt.category,
                counts_in_average: mt.counts_in_average,
                numeric_value: mt.numeric_value,
                weight,
                work_type,
                topic: validate_optional_text(
                    "topic",
                    input.topic.as_deref().unwrap_or(""),
                    MAX_TITLE_LEN,
                )?,
                comment: validate_optional_text(
                    "comment",
                    input.comment.as_deref().unwrap_or(""),
                    MAX_TEXT_LEN,
                )?,
                teacher: validate_optional_text(
                    "teacher",
                    input.teacher.as_deref().unwrap_or(&subject.teacher),
                    MAX_TITLE_LEN,
                )?,
                source,
                created_at: now,
                updated_at: now,
            });
        }
        self.db.insert_grades_batch(&entries, source)?;
        self.invalidate_stats_cache();
        let pid = entries.first().map(|e| e.profile_id);
        let undo_cmd = UndoCommand::BatchGradesAdded {
            grade_ids: entries.iter().map(|e| e.id).collect(),
            grades: entries.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            pid,
            "grade",
            None,
            "batch_create",
            format!("Массово добавлено оценок: {}", entries.len()),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "grade");
        Ok(entries)
    }

    pub fn update_grade(
        &self,
        id: Uuid,
        input: UpdateGradeInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<GradeEntry> {
        let before = self
            .db
            .get_grade(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "grade".into(),
                id: id.to_string(),
            })?;
        let mut after = before.clone();
        if let Some(sid) = input.subject_id {
            self.verify_subject_in_profile(after.profile_id, sid)?;
            after.subject_id = sid;
        }
        if let Some(date) = input.date {
            after.date = date;
        }
        if let Some(time) = input.time {
            after.time = time;
        }
        if let Some(mt_id) = input.mark_type_id {
            let mt = self
                .db
                .get_mark_type(mt_id)?
                .ok_or_else(|| DomainError::NotFound {
                    entity: "mark_type".into(),
                    id: mt_id.to_string(),
                })?;
            after.mark_type_id = mt.id;
            after.mark_code = mt.code;
            after.mark_label = mt.short_label;
            after.mark_category = mt.category;
            after.counts_in_average = mt.counts_in_average;
            after.numeric_value = mt.numeric_value;
        }
        if let Some(w) = input.weight {
            after.weight = validate_weight(w)?;
        }
        if let Some(wt) = input.work_type {
            after.work_type = wt;
        }
        if let Some(topic) = input.topic {
            after.topic = validate_optional_text("topic", &topic, MAX_TITLE_LEN)?;
        }
        if let Some(comment) = input.comment {
            after.comment = validate_optional_text("comment", &comment, MAX_TEXT_LEN)?;
        }
        if let Some(teacher) = input.teacher {
            after.teacher = validate_optional_text("teacher", &teacher, MAX_TITLE_LEN)?;
        }
        after.updated_at = Utc::now();
        self.db.update_grade(&after, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::GradeUpdated {
            before,
            after: after.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(after.profile_id),
            "grade",
            Some(after.id),
            "update",
            format!("Изменена отметка «{}» ({})", after.mark_label, after.date),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "grade");
        Ok(after)
    }

    pub fn copy_grade_to_date(
        &self,
        grade_id: Uuid,
        target_date: NaiveDate,
        target_subject_id: Option<Uuid>,
        source: RecordSource,
    ) -> AppResult<GradeEntry> {
        let orig = self.get_grade(grade_id)?;
        self.add_grade(
            AddGradeInput {
                profile_id: orig.profile_id,
                subject_id: target_subject_id.unwrap_or(orig.subject_id),
                date: target_date,
                time: orig.time,
                mark_type_id: Some(orig.mark_type_id),
                mark_code: None,
                numeric_grade: None,
                weight: Some(orig.weight),
                work_type: Some(orig.work_type),
                topic: Some(orig.topic),
                comment: Some(orig.comment),
                teacher: Some(orig.teacher),
            },
            source,
            None,
        )
    }

    pub fn move_grade_to_date(
        &self,
        grade_id: Uuid,
        target_date: NaiveDate,
        target_subject_id: Option<Uuid>,
        source: RecordSource,
    ) -> AppResult<GradeEntry> {
        self.update_grade(
            grade_id,
            UpdateGradeInput {
                subject_id: target_subject_id,
                date: Some(target_date),
                ..Default::default()
            },
            source,
            None,
        )
    }

    pub fn delete_grade(
        &self,
        id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<GradeEntry> {
        let existing = self.get_grade(id)?;
        self.db.delete_grade(id, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::GradeDeleted {
            grade: existing.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(existing.profile_id),
            "grade",
            Some(existing.id),
            "delete",
            format!("Удалена отметка «{}» ({})", existing.mark_label, existing.date),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "grade");
        Ok(existing)
    }

    pub fn get_grade(&self, id: Uuid) -> AppResult<GradeEntry> {
        self.db
            .get_grade(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "grade".into(),
                id: id.to_string(),
            }
            .into())
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
    ) -> AppResult<Vec<GradeEntry>> {
        Ok(self.db.list_grades(
            profile_id,
            subject_id,
            from_date,
            to_date,
            work_type,
            mark_type_id,
            limit,
            offset,
        )?)
    }

    // ========================================================================
    // Attendance
    // ========================================================================

    fn mark_type_for_attendance(&self, profile_id: Uuid, status: &AttendanceStatus) -> AppResult<Uuid> {
        match status {
            AttendanceStatus::Absent => Ok(builtin_marks::ABSENT_ID),
            AttendanceStatus::Excused => Ok(builtin_marks::EXCUSED_ID),
            AttendanceStatus::Present => Ok(builtin_marks::PRESENT_ID),
            AttendanceStatus::Custom(code) => {
                let marks = self.db.list_mark_types(Some(profile_id))?;
                marks
                    .into_iter()
                    .find(|m| m.code.eq_ignore_ascii_case(code))
                    .map(|m| m.id)
                    .ok_or_else(|| {
                        DomainError::validation(
                            "status",
                            format!("unknown custom attendance mark code '{code}'"),
                        )
                        .into()
                    })
            }
        }
    }

    pub fn add_attendance(
        &self,
        input: AddAttendanceInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<AttendanceEntry> {
        let subject = self.verify_subject_in_profile(input.profile_id, input.subject_id)?;
        let mark_type_id = self.mark_type_for_attendance(input.profile_id, &input.status)?;
        let now = Utc::now();
        let entry = AttendanceEntry {
            id: Uuid::new_v4(),
            profile_id: input.profile_id,
            subject_id: subject.id,
            date: input.date,
            time: input.time,
            status: input.status,
            mark_type_id,
            reason: validate_optional_text(
                "reason",
                input.reason.as_deref().unwrap_or(""),
                MAX_TITLE_LEN,
            )?,
            comment: validate_optional_text(
                "comment",
                input.comment.as_deref().unwrap_or(""),
                MAX_TEXT_LEN,
            )?,
            source,
            created_at: now,
            updated_at: now,
        };
        self.db.insert_attendance(&entry)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::AttendanceAdded {
            entry: entry.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(entry.profile_id),
            "attendance",
            Some(entry.id),
            "create",
            format!(
                "Посещаемость ({}) по предмету «{}» ({})",
                entry.status.as_db_str(),
                subject.name,
                entry.date
            ),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "attendance");
        Ok(entry)
    }

    pub fn update_attendance(
        &self,
        id: Uuid,
        input: UpdateAttendanceInput,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<AttendanceEntry> {
        let mut entry = self
            .db
            .get_attendance(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "attendance".into(),
                id: id.to_string(),
            })?;
        if let Some(d) = input.date {
            entry.date = d;
        }
        if let Some(t) = input.time {
            entry.time = t;
        }
        if let Some(st) = input.status {
            entry.mark_type_id = self.mark_type_for_attendance(entry.profile_id, &st)?;
            entry.status = st;
        }
        if let Some(r) = input.reason {
            entry.reason = validate_optional_text("reason", &r, MAX_TITLE_LEN)?;
        }
        if let Some(c) = input.comment {
            entry.comment = validate_optional_text("comment", &c, MAX_TEXT_LEN)?;
        }
        entry.updated_at = Utc::now();
        self.db.update_attendance(&entry, source)?;
        self.invalidate_stats_cache();
        self.log_activity(
            source,
            audit,
            Some(entry.profile_id),
            "attendance",
            Some(entry.id),
            "update",
            format!("Обновлена запись посещаемости ({})", entry.date),
            None,
        );
        self.sync.notify_local_change(source, "attendance");
        Ok(entry)
    }

    pub fn delete_attendance(
        &self,
        id: Uuid,
        source: RecordSource,
        audit: Option<&AuditMeta>,
    ) -> AppResult<AttendanceEntry> {
        let existing = self
            .db
            .get_attendance(id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "attendance".into(),
                id: id.to_string(),
            })?;
        self.db.delete_attendance(id, source)?;
        self.invalidate_stats_cache();
        let undo_cmd = UndoCommand::AttendanceDeleted {
            entry: existing.clone(),
        };
        if source == RecordSource::Gui {
            self.undo.push(undo_cmd.clone());
        }
        self.log_activity(
            source,
            audit,
            Some(existing.profile_id),
            "attendance",
            Some(existing.id),
            "delete",
            format!("Удалена запись посещаемости ({})", existing.date),
            Some(serde_json::to_value(&undo_cmd)?),
        );
        self.sync.notify_local_change(source, "attendance");
        Ok(existing)
    }

    pub fn list_attendance(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
    ) -> AppResult<Vec<AttendanceEntry>> {
        Ok(self
            .db
            .list_attendance(profile_id, subject_id, from_date, to_date)?)
    }

    // ========================================================================
    // Journal Views & Summaries
    // ========================================================================

    pub fn get_monthly_journal(
        &self,
        profile_id: Uuid,
        year: i32,
        month: u32,
        today: NaiveDate,
    ) -> AppResult<JournalMatrix> {
        let (start_date, end_date) = month_bounds(year, month).ok_or_else(|| {
            DomainError::validation("month", format!("invalid year/month {year}-{month}"))
        })?;
        let holidays = self.db.list_holidays(Some(profile_id))?;
        let days = build_month_calendar(year, month, today, &holidays);
        self.build_journal_matrix(profile_id, start_date, end_date, days)
    }

    pub fn get_journal_for_range(
        &self,
        profile_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
        today: NaiveDate,
    ) -> AppResult<JournalMatrix> {
        journal_domain::validate_date_range(start_date, end_date)?;
        let holidays = self.db.list_holidays(Some(profile_id))?;
        let days = build_date_days(start_date, end_date, today, &holidays);
        self.build_journal_matrix(profile_id, start_date, end_date, days)
    }

    fn build_journal_matrix(
        &self,
        profile_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
        days: Vec<CalendarDay>,
    ) -> AppResult<JournalMatrix> {
        let settings = self.db.load_settings()?;
        let subjects = self.db.list_subjects(profile_id, false)?;
        let grades = self.db.list_grades(
            profile_id,
            None,
            Some(start_date),
            Some(end_date),
            None,
            None,
            None,
            None,
        )?;
        let attendance =
            self.db
                .list_attendance(profile_id, None, Some(start_date), Some(end_date))?;

        let mut grades_by_subject: HashMap<Uuid, Vec<GradeEntry>> = HashMap::new();
        for g in &grades {
            grades_by_subject.entry(g.subject_id).or_default().push(g.clone());
        }
        let mut att_by_subject: HashMap<Uuid, Vec<AttendanceEntry>> = HashMap::new();
        for a in &attendance {
            att_by_subject.entry(a.subject_id).or_default().push(a.clone());
        }

        let mut rows = Vec::with_capacity(subjects.len());
        for subject in subjects {
            let subj_grades = grades_by_subject.remove(&subject.id).unwrap_or_default();
            let subj_att = att_by_subject.remove(&subject.id).unwrap_or_default();

            let avg = calculate_average(&subj_grades, settings.average_mode);
            let avg_fmt = format_average(avg, settings.average_precision);
            let numeric_count = subj_grades
                .iter()
                .filter(|g| g.counts_in_average && g.numeric_value.is_some())
                .count();
            let absence_count = subj_att
                .iter()
                .filter(|a| a.status == AttendanceStatus::Absent)
                .count();
            let excused_count = subj_att
                .iter()
                .filter(|a| a.status == AttendanceStatus::Excused)
                .count();

            let mut cells: HashMap<NaiveDate, JournalCell> = HashMap::new();
            for g in subj_grades {
                cells
                    .entry(g.date)
                    .or_insert_with(|| JournalCell {
                        subject_id: subject.id,
                        date: g.date,
                        grades: Vec::new(),
                        attendance: Vec::new(),
                    })
                    .grades
                    .push(g);
            }
            for a in subj_att {
                cells
                    .entry(a.date)
                    .or_insert_with(|| JournalCell {
                        subject_id: subject.id,
                        date: a.date,
                        grades: Vec::new(),
                        attendance: Vec::new(),
                    })
                    .attendance
                    .push(a);
            }

            rows.push(JournalSubjectRow {
                subject,
                cells_by_date: cells,
                average: avg,
                average_formatted: avg_fmt,
                numeric_grade_count: numeric_count,
                absence_count,
                excused_count,
            });
        }

        let overall_avg = calculate_average(&grades, settings.average_mode);
        let overall_fmt = format_average(overall_avg, settings.average_precision);

        Ok(JournalMatrix {
            profile_id,
            start_date,
            end_date,
            days,
            rows,
            overall_average: overall_avg,
            overall_average_formatted: overall_fmt,
            average_mode: settings.average_mode,
        })
    }

    pub fn get_period_summary(
        &self,
        profile_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> AppResult<PeriodSummary> {
        journal_domain::validate_date_range(start_date, end_date)?;
        let settings = self.db.load_settings()?;
        let subjects = self.db.list_subjects(profile_id, true)?;
        let subj_names: HashMap<Uuid, String> =
            subjects.into_iter().map(|s| (s.id, s.name)).collect();

        let grades = self.db.list_grades(
            profile_id,
            None,
            Some(start_date),
            Some(end_date),
            None,
            None,
            None,
            None,
        )?;
        let attendance =
            self.db
                .list_attendance(profile_id, None, Some(start_date), Some(end_date))?;

        let avg = calculate_average(&grades, settings.average_mode);
        let avg_fmt = format_average(avg, settings.average_precision);
        let numeric_grades = grades
            .iter()
            .filter(|g| g.counts_in_average && g.numeric_value.is_some())
            .count();
        let absences = attendance
            .iter()
            .filter(|a| a.status == AttendanceStatus::Absent)
            .count();
        let excused_absences = attendance
            .iter()
            .filter(|a| a.status == AttendanceStatus::Excused)
            .count();
        let present_records = attendance
            .iter()
            .filter(|a| a.status == AttendanceStatus::Present)
            .count();

        let mut by_subj: HashMap<Uuid, (Vec<GradeEntry>, usize, usize)> = HashMap::new();
        for g in &grades {
            by_subj
                .entry(g.subject_id)
                .or_insert_with(|| (Vec::new(), 0, 0))
                .0
                .push(g.clone());
        }
        for a in &attendance {
            let entry = by_subj
                .entry(a.subject_id)
                .or_insert_with(|| (Vec::new(), 0, 0));
            match a.status {
                AttendanceStatus::Absent => entry.1 += 1,
                AttendanceStatus::Excused => entry.2 += 1,
                _ => {}
            }
        }

        let mut grades_by_subject: Vec<SubjectSummaryItem> = by_subj
            .into_iter()
            .map(|(sid, (gs, abs, exc))| {
                let s_avg = calculate_average(&gs, settings.average_mode);
                SubjectSummaryItem {
                    subject_id: sid,
                    subject_name: subj_names
                        .get(&sid)
                        .cloned()
                        .unwrap_or_else(|| sid.to_string()),
                    grade_count: gs.len(),
                    average: s_avg,
                    average_formatted: format_average(s_avg, settings.average_precision),
                    absences: abs,
                    excused: exc,
                }
            })
            .collect();
        grades_by_subject.sort_by(|a, b| a.subject_name.cmp(&b.subject_name));

        Ok(PeriodSummary {
            profile_id,
            start_date,
            end_date,
            total_grades: grades.len(),
            numeric_grades,
            average: avg,
            average_formatted: avg_fmt,
            absences,
            excused_absences,
            present_records,
            grades_by_subject,
        })
    }

    pub fn get_daily_summary(&self, profile_id: Uuid, date: NaiveDate) -> AppResult<PeriodSummary> {
        self.get_period_summary(profile_id, date, date)
    }

    pub fn get_weekly_summary(
        &self,
        profile_id: Uuid,
        anchor_date: NaiveDate,
    ) -> AppResult<PeriodSummary> {
        let settings = self.db.load_settings()?;
        let (start, end) = week_bounds(anchor_date, settings.week_start);
        self.get_period_summary(profile_id, start, end)
    }

    pub fn get_monthly_summary(
        &self,
        profile_id: Uuid,
        year: i32,
        month: u32,
    ) -> AppResult<PeriodSummary> {
        let (start, end) = month_bounds(year, month).ok_or_else(|| {
            DomainError::validation("month", format!("invalid year/month {year}-{month}"))
        })?;
        self.get_period_summary(profile_id, start, end)
    }
}
