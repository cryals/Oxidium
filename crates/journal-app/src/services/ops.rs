use chrono::{NaiveDate, Utc};
use journal_domain::{
    ActivityLogEntry, AttendanceEntry, DomainError, GradeEntry, NotificationItem, NotificationKind,
    Profile, RecordSource, SearchResultItem, Subject, Task, WorkType, format_average,
};
use journal_storage::SearchFilter;
use rust_decimal::Decimal;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::secret::redact_secrets;
use crate::services::{
    AddGradeInput, AppServices, CreateSubjectInput, CreateTaskInput, StatisticsFilter,
};
use crate::undo::UndoCommand;

#[derive(Debug, Clone)]
pub struct AuditMeta {
    pub client_identity: Option<String>,
    pub tool_name: Option<String>,
    pub correlation_id: String,
}

impl AuditMeta {
    pub fn mcp(client: Option<String>, tool: impl Into<String>) -> Self {
        Self {
            client_identity: client,
            tool_name: Some(tool.into()),
            correlation_id: Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExportBundle {
    pub schema_version: u32,
    pub exported_at: chrono::DateTime<Utc>,
    pub profile: Profile,
    pub subjects: Vec<Subject>,
    pub grades: Vec<GradeEntry>,
    pub attendance: Vec<AttendanceEntry>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub format: String,
    pub subjects_count: usize,
    pub grades_count: usize,
    pub attendance_count: usize,
    pub tasks_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessagePreview {
    pub id: i64,
    pub date: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramImportPreview {
    pub chat_name: String,
    pub total_messages: usize,
    pub non_empty_notes: Vec<TelegramMessagePreview>,
    pub notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupArchiveInfo {
    pub path: PathBuf,
    pub created_at: chrono::DateTime<Utc>,
    pub size_bytes: u64,
    pub sha256: String,
}

impl AppServices {
    // ========================================================================
    // Search
    // ========================================================================

    pub fn search_global(
        &self,
        query: &str,
        filter: &SearchFilter,
    ) -> AppResult<Vec<SearchResultItem>> {
        Ok(self.db.search_global(query, filter)?)
    }

    // ========================================================================
    // Activity Log & Undo / Redo
    // ========================================================================

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn log_activity(
        &self,
        source: RecordSource,
        audit: Option<&AuditMeta>,
        profile_id: Option<Uuid>,
        entity_type: &str,
        entity_id: Option<Uuid>,
        operation_type: &str,
        summary: String,
        undo_payload: Option<serde_json::Value>,
    ) {
        let entry = ActivityLogEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            client_identity: audit.and_then(|a| a.client_identity.clone()),
            tool_name: audit.and_then(|a| a.tool_name.clone()),
            profile_id,
            entity_type: entity_type.to_string(),
            entity_id,
            operation_type: operation_type.to_string(),
            summary: redact_secrets(&summary),
            result_status: "ok".into(),
            source,
            correlation_id: audit
                .map(|a| a.correlation_id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            undo_payload,
            undone: false,
        };
        if let Err(e) = self.db.insert_activity_log(&entry) {
            tracing::warn!(error = %e, "failed to record activity log entry");
        }
    }

    pub fn list_activity_log(
        &self,
        profile_id: Option<Uuid>,
        source: Option<RecordSource>,
        limit: usize,
    ) -> AppResult<Vec<ActivityLogEntry>> {
        Ok(self.db.list_activity_log(profile_id, source, limit)?)
    }

    pub fn undo_last(&self) -> AppResult<bool> {
        let Some(cmd) = self.undo.pop_undo() else {
            return Ok(false);
        };
        self.apply_undo_command(&cmd)?;
        self.undo.push_redo(cmd);
        Ok(true)
    }

    pub fn redo_last(&self) -> AppResult<bool> {
        let Some(cmd) = self.undo.pop_redo() else {
            return Ok(false);
        };
        self.apply_redo_command(&cmd)?;
        self.undo.push_undo_without_clearing_redo(cmd);
        Ok(true)
    }

    pub fn undo_activity_entry(&self, activity_id: Uuid) -> AppResult<bool> {
        let entry = self
            .db
            .get_activity_log_entry(activity_id)?
            .ok_or_else(|| DomainError::NotFound {
                entity: "activity_log".into(),
                id: activity_id.to_string(),
            })?;
        if entry.undone {
            return Ok(false);
        }
        let Some(payload) = entry.undo_payload else {
            return Err(DomainError::InvalidState(
                "this activity log action does not support undo".into(),
            )
            .into());
        };
        let cmd: UndoCommand = serde_json::from_value(payload)?;
        self.apply_undo_command(&cmd)?;
        self.db.mark_activity_undone(activity_id)?;
        Ok(true)
    }

    fn apply_undo_command(&self, cmd: &UndoCommand) -> AppResult<()> {
        match cmd {
            UndoCommand::GradeAdded { grade } => {
                let _ = self.db.delete_grade(grade.id, RecordSource::Gui);
            }
            UndoCommand::GradeUpdated { before, .. } => {
                self.db.update_grade(before, RecordSource::Gui)?;
            }
            UndoCommand::GradeDeleted { grade } => {
                self.db.insert_grade(grade)?;
            }
            UndoCommand::AttendanceAdded { entry } => {
                let _ = self.db.delete_attendance(entry.id, RecordSource::Gui);
            }
            UndoCommand::AttendanceDeleted { entry } => {
                self.db.insert_attendance(entry)?;
            }
            UndoCommand::TaskCreated { task } => {
                let _ = self.db.delete_task(task.id, RecordSource::Gui);
            }
            UndoCommand::TaskUpdated { before, .. } => {
                self.db.update_task(before, RecordSource::Gui)?;
            }
            UndoCommand::TaskDeleted { task } => {
                self.db.insert_task(task, RecordSource::Gui)?;
            }
            UndoCommand::BatchGradesAdded { grade_ids, .. } => {
                for id in grade_ids {
                    let _ = self.db.delete_grade(*id, RecordSource::Gui);
                }
            }
        }
        self.invalidate_stats_cache();
        self.sync.notify_local_change(RecordSource::Gui, "undo");
        Ok(())
    }

    fn apply_redo_command(&self, cmd: &UndoCommand) -> AppResult<()> {
        match cmd {
            UndoCommand::GradeAdded { grade } => {
                self.db.insert_grade(grade)?;
            }
            UndoCommand::GradeUpdated { after, .. } => {
                self.db.update_grade(after, RecordSource::Gui)?;
            }
            UndoCommand::GradeDeleted { grade } => {
                self.db.delete_grade(grade.id, RecordSource::Gui)?;
            }
            UndoCommand::AttendanceAdded { entry } => {
                self.db.insert_attendance(entry)?;
            }
            UndoCommand::AttendanceDeleted { entry } => {
                self.db.delete_attendance(entry.id, RecordSource::Gui)?;
            }
            UndoCommand::TaskCreated { task } => {
                self.db.insert_task(task, RecordSource::Gui)?;
            }
            UndoCommand::TaskUpdated { after, .. } => {
                self.db.update_task(after, RecordSource::Gui)?;
            }
            UndoCommand::TaskDeleted { task } => {
                self.db.delete_task(task.id, RecordSource::Gui)?;
            }
            UndoCommand::BatchGradesAdded { grades, .. } => {
                self.db.insert_grades_batch(grades, RecordSource::Gui)?;
            }
        }
        self.invalidate_stats_cache();
        self.sync.notify_local_change(RecordSource::Gui, "redo");
        Ok(())
    }

    // ========================================================================
    // Notifications & Reminders
    // ========================================================================

    pub fn list_notifications(
        &self,
        include_dismissed: bool,
        limit: usize,
    ) -> AppResult<Vec<NotificationItem>> {
        Ok(self.db.list_notifications(include_dismissed, limit)?)
    }

    pub fn mark_notification_read(&self, id: Uuid) -> AppResult<()> {
        self.db.mark_notification_read(id)?;
        Ok(())
    }

    pub fn dismiss_notification(&self, id: Uuid) -> AppResult<()> {
        self.db.dismiss_notification(id)?;
        Ok(())
    }

    pub fn mark_all_notifications_read(&self) -> AppResult<()> {
        self.db.mark_all_notifications_read()?;
        Ok(())
    }

    /// Evaluates due reminders and overdue tasks, inserts internal notifications,
    /// and dispatches OS notifications when enabled.
    pub fn process_due_reminders(&self) -> AppResult<Vec<NotificationItem>> {
        let settings = self.db.load_settings()?;
        if !settings.reminders_enabled {
            return Ok(Vec::new());
        }
        let now = Utc::now();
        let due_reminders = self.db.list_due_reminders(now)?;
        let mut fired = Vec::new();

        for rem in due_reminders {
            if let Some(task) = self.db.get_task(rem.task_id)?
                && task.completed_at.is_none()
                && !task.is_archived
            {
                let notif = NotificationItem {
                    id: Uuid::new_v4(),
                    profile_id: Some(task.profile_id),
                    kind: if task.is_overdue(now) {
                        NotificationKind::TaskOverdue
                    } else {
                        NotificationKind::Reminder
                    },
                    title: format!("Напоминание: {}", task.title),
                    body: task
                        .due_at
                        .map(|d| format!("Дедлайн: {}", d.format("%d.%m.%Y %H:%M UTC")))
                        .unwrap_or_default(),
                    entity_type: Some("task".into()),
                    entity_id: Some(task.id),
                    is_read: false,
                    is_dismissed: false,
                    created_at: now,
                };
                self.db.insert_notification(&notif)?;
                if settings.os_notifications_enabled {
                    send_os_notification(&notif.title, &notif.body);
                }
                fired.push(notif);
            }
            self.db.mark_reminder_fired(rem.id)?;
        }

        Ok(fired)
    }

    // ========================================================================
    // Export (JSON, CSV, XLSX, Markdown, Full Backup ZIP)
    // ========================================================================

    pub fn export_profile_json(&self, profile_id: Uuid, dest: impl AsRef<Path>) -> AppResult<()> {
        let bundle = ProfileExportBundle {
            schema_version: 1,
            exported_at: Utc::now(),
            profile: self.get_profile(profile_id)?,
            subjects: self.db.list_subjects(profile_id, true)?,
            grades: self
                .db
                .list_grades(profile_id, None, None, None, None, None, None, None)?,
            attendance: self.db.list_attendance(profile_id, None, None, None)?,
            tasks: self.db.list_tasks(profile_id, None, None, None, true)?,
        };
        let json = serde_json::to_string_pretty(&bundle)?;
        std::fs::write(dest, json)?;
        Ok(())
    }

    pub fn export_grades_csv(&self, profile_id: Uuid, dest: impl AsRef<Path>) -> AppResult<()> {
        let subjects: HashMap<Uuid, String> = self
            .db
            .list_subjects(profile_id, true)?
            .into_iter()
            .map(|s| (s.id, s.name))
            .collect();
        let grades = self
            .db
            .list_grades(profile_id, None, None, None, None, None, None, None)?;
        let mut wtr = csv::Writer::from_path(dest)?;
        wtr.write_record([
            "date",
            "subject",
            "mark",
            "numeric_value",
            "weight",
            "work_type",
            "topic",
            "comment",
            "teacher",
            "source",
        ])?;
        for g in grades {
            let subj = subjects
                .get(&g.subject_id)
                .cloned()
                .unwrap_or_else(|| g.subject_id.to_string());
            wtr.write_record([
                g.date.format("%Y-%m-%d").to_string(),
                subj,
                g.mark_label,
                g.numeric_value.map(|d| d.to_string()).unwrap_or_default(),
                g.weight.to_string(),
                g.work_type.as_str().to_string(),
                g.topic,
                g.comment,
                g.teacher,
                g.source.as_str().to_string(),
            ])?;
        }
        wtr.flush()?;
        Ok(())
    }

    pub fn export_profile_xlsx(&self, profile_id: Uuid, dest: impl AsRef<Path>) -> AppResult<()> {
        let subjects = self.db.list_subjects(profile_id, true)?;
        let subj_map: HashMap<Uuid, String> =
            subjects.iter().map(|s| (s.id, s.name.clone())).collect();
        let grades = self
            .db
            .list_grades(profile_id, None, None, None, None, None, None, None)?;
        let attendance = self.db.list_attendance(profile_id, None, None, None)?;
        let tasks = self.db.list_tasks(profile_id, None, None, None, true)?;
        let stats = self.get_statistics(&StatisticsFilter {
            profile_id,
            ..Default::default()
        })?;

        let mut wb = Workbook::new();

        // Sheet 1: Grades
        let ws_g = wb
            .add_worksheet()
            .set_name("Grades")
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
        for (col, hdr) in [
            "Date",
            "Subject",
            "Mark",
            "Numeric",
            "Weight",
            "Work Type",
            "Topic",
            "Comment",
        ]
        .iter()
        .enumerate()
        {
            ws_g.write_string(0, col as u16, *hdr)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }
        for (row, g) in grades.iter().enumerate() {
            let r = (row + 1) as u32;
            let sname = subj_map
                .get(&g.subject_id)
                .map(String::as_str)
                .unwrap_or("");
            ws_g.write_string(r, 0, g.date.format("%Y-%m-%d").to_string())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 1, sname)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 2, &g.mark_label)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(
                r,
                3,
                g.numeric_value.map(|d| d.to_string()).unwrap_or_default(),
            )
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 4, g.weight.to_string())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 5, g.work_type.as_str())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 6, &g.topic)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_g.write_string(r, 7, &g.comment)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }

        // Sheet 2: Attendance
        let ws_a = wb
            .add_worksheet()
            .set_name("Attendance")
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
        for (col, hdr) in ["Date", "Subject", "Status", "Reason", "Comment"]
            .iter()
            .enumerate()
        {
            ws_a.write_string(0, col as u16, *hdr)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }
        for (row, a) in attendance.iter().enumerate() {
            let r = (row + 1) as u32;
            let sname = subj_map
                .get(&a.subject_id)
                .map(String::as_str)
                .unwrap_or("");
            ws_a.write_string(r, 0, a.date.format("%Y-%m-%d").to_string())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_a.write_string(r, 1, sname)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_a.write_string(r, 2, a.status.as_db_str())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_a.write_string(r, 3, &a.reason)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_a.write_string(r, 4, &a.comment)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }

        // Sheet 3: Statistics
        let ws_s = wb
            .add_worksheet()
            .set_name("Statistics")
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
        for (col, hdr) in ["Subject", "Average", "Grades", "Absences", "Excused"]
            .iter()
            .enumerate()
        {
            ws_s.write_string(0, col as u16, *hdr)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }
        for (row, st) in stats.subject_stats.iter().enumerate() {
            let r = (row + 1) as u32;
            ws_s.write_string(r, 0, &st.subject_name)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_s.write_string(r, 1, &st.average_formatted)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_s.write_number(r, 2, st.numeric_grade_count as f64)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_s.write_number(r, 3, st.absences as f64)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_s.write_number(r, 4, st.excused_absences as f64)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }

        // Sheet 4: Tasks
        let ws_t = wb
            .add_worksheet()
            .set_name("Tasks")
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
        for (col, hdr) in ["Title", "Subject", "Priority", "Due", "Completed"]
            .iter()
            .enumerate()
        {
            ws_t.write_string(0, col as u16, *hdr)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }
        for (row, t) in tasks.iter().enumerate() {
            let r = (row + 1) as u32;
            let sname = t
                .subject_id
                .and_then(|id| subj_map.get(&id))
                .map(String::as_str)
                .unwrap_or("");
            ws_t.write_string(r, 0, &t.title)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_t.write_string(r, 1, sname)
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_t.write_string(r, 2, t.priority.as_str())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_t.write_string(r, 3, t.due_at.map(|d| d.to_rfc3339()).unwrap_or_default())
                .map_err(|e| AppError::Xlsx(e.to_string()))?;
            ws_t.write_string(
                r,
                4,
                t.completed_at.map(|d| d.to_rfc3339()).unwrap_or_default(),
            )
            .map_err(|e| AppError::Xlsx(e.to_string()))?;
        }

        wb.save(dest).map_err(|e| AppError::Xlsx(e.to_string()))?;
        Ok(())
    }

    pub fn export_profile_markdown(
        &self,
        profile_id: Uuid,
        dest: impl AsRef<Path>,
    ) -> AppResult<()> {
        let profile = self.get_profile(profile_id)?;
        let stats = self.get_statistics(&StatisticsFilter {
            profile_id,
            ..Default::default()
        })?;
        let settings = self.db.load_settings()?;
        let tasks = self.db.list_tasks(profile_id, None, None, None, false)?;

        let mut md = String::new();
        md.push_str(&format!("# Oxidium Export — {}\n\n", profile.name));
        md.push_str(&format!(
            "- **Overall Average**: {}\n- **Numeric Grades**: {}\n- **Absences**: {} (Excused: {})\n\n",
            format_average(stats.overall_average, settings.average_precision),
            stats.numeric_grade_count,
            stats.absences,
            stats.excused_absences
        ));
        md.push_str("## Subjects\n\n| Subject | Average | Grades | Absences | Excused |\n|---|---|---|---|---|\n");
        for s in &stats.subject_stats {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                s.subject_name,
                s.average_formatted,
                s.numeric_grade_count,
                s.absences,
                s.excused_absences
            ));
        }
        md.push_str("\n## Active Tasks\n\n");
        for t in tasks {
            let mark = if t.is_completed() { "x" } else { " " };
            let due = t
                .due_at
                .map(|d| format!(" (due {})", d.format("%Y-%m-%d")))
                .unwrap_or_default();
            md.push_str(&format!("- [{mark}] **{}**{due}\n", t.title));
        }
        std::fs::write(dest, md)?;
        Ok(())
    }

    // ========================================================================
    // Import (JSON, CSV, Telegram Desktop JSON preview)
    // ========================================================================

    pub fn preview_json_import(&self, path: impl AsRef<Path>) -> AppResult<ImportPreview> {
        let raw = std::fs::read_to_string(path)?;
        let bundle: ProfileExportBundle = serde_json::from_str(&raw)?;
        Ok(ImportPreview {
            format: "oxidium_json".into(),
            subjects_count: bundle.subjects.len(),
            grades_count: bundle.grades.len(),
            attendance_count: bundle.attendance.len(),
            tasks_count: bundle.tasks.len(),
            warnings: vec![],
        })
    }

    pub fn import_json_into_profile(
        &self,
        profile_id: Uuid,
        path: impl AsRef<Path>,
    ) -> AppResult<ImportPreview> {
        let raw = std::fs::read_to_string(path)?;
        let bundle: ProfileExportBundle = serde_json::from_str(&raw)?;
        let mut subject_map: HashMap<Uuid, Uuid> = HashMap::new();
        let mut warnings = Vec::new();

        for s in &bundle.subjects {
            let existing = self.find_subject(profile_id, &s.name)?;
            if let Some(ex) = existing.exact {
                subject_map.insert(s.id, ex.id);
            } else {
                let created = self.create_subject(
                    CreateSubjectInput {
                        profile_id,
                        name: s.name.clone(),
                        short_name: Some(s.short_name.clone()),
                        teacher: Some(s.teacher.clone()),
                        color: Some(s.color.clone()),
                        icon: Some(s.icon.clone()),
                        description: Some(s.description.clone()),
                        academic_period_id: None,
                        target_average: s.target_average,
                    },
                    RecordSource::Import,
                    None,
                )?;
                subject_map.insert(s.id, created.id);
            }
        }

        let mut grades_imported = 0usize;
        for g in &bundle.grades {
            let Some(&new_sid) = subject_map.get(&g.subject_id) else {
                warnings.push(format!("Skipped grade {} with unmapped subject", g.id));
                continue;
            };
            let _ = self.add_grade(
                AddGradeInput {
                    profile_id,
                    subject_id: new_sid,
                    date: g.date,
                    time: g.time,
                    mark_type_id: Some(g.mark_type_id),
                    mark_code: Some(g.mark_code.clone()),
                    numeric_grade: None,
                    weight: Some(g.weight),
                    work_type: Some(g.work_type),
                    topic: Some(g.topic.clone()),
                    comment: Some(g.comment.clone()),
                    teacher: Some(g.teacher.clone()),
                },
                RecordSource::Import,
                None,
            );
            grades_imported += 1;
        }

        let mut tasks_imported = 0usize;
        for t in &bundle.tasks {
            let mapped_sid = t.subject_id.and_then(|old| subject_map.get(&old).copied());
            let _ = self.create_task(
                CreateTaskInput {
                    profile_id,
                    board_id: None,
                    column_id: None,
                    subject_id: mapped_sid,
                    title: t.title.clone(),
                    description: Some(t.description.clone()),
                    priority: Some(t.priority),
                    color: t.color.clone(),
                    start_date: t.start_date,
                    due_at: t.due_at,
                    notes: Some(t.notes.clone()),
                    tag_names: t.tags.iter().map(|tg| tg.name.clone()).collect(),
                    badge_names: t.badges.iter().map(|b| b.name.clone()).collect(),
                    checklist_titles: t.checklist.iter().map(|c| c.title.clone()).collect(),
                    reminder_offsets: vec![],
                },
                RecordSource::Import,
                None,
            );
            tasks_imported += 1;
        }

        Ok(ImportPreview {
            format: "oxidium_json".into(),
            subjects_count: subject_map.len(),
            grades_count: grades_imported,
            attendance_count: 0,
            tasks_count: tasks_imported,
            warnings,
        })
    }

    pub fn import_grades_csv(
        &self,
        profile_id: Uuid,
        path: impl AsRef<Path>,
        dry_run: bool,
    ) -> AppResult<ImportPreview> {
        let mut rdr = csv::Reader::from_path(path)?;
        let mut grades_count = 0usize;
        let mut subjects_seen = std::collections::HashSet::new();
        let mut warnings = Vec::new();
        let mut pending = Vec::new();

        for (idx, result) in rdr.records().enumerate() {
            let rec = result?;
            let date_str = rec.get(0).unwrap_or("").trim();
            let subj_name = rec.get(1).unwrap_or("").trim();
            let mark = rec.get(2).unwrap_or("").trim();
            let weight_str = rec.get(4).unwrap_or("1").trim();
            let wt_str = rec.get(5).unwrap_or("class_answer").trim();
            let topic = rec.get(6).unwrap_or("").trim().to_string();
            let comment = rec.get(7).unwrap_or("").trim().to_string();

            let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
                warnings.push(format!("Row {}: invalid date '{}'", idx + 2, date_str));
                continue;
            };
            if subj_name.is_empty() || mark.is_empty() {
                warnings.push(format!("Row {}: missing subject or mark", idx + 2));
                continue;
            }
            subjects_seen.insert(subj_name.to_string());
            grades_count += 1;
            if !dry_run {
                let weight = weight_str.parse::<Decimal>().unwrap_or(Decimal::ONE);
                let wt = WorkType::parse(wt_str).unwrap_or(WorkType::Other);
                pending.push((
                    date,
                    subj_name.to_string(),
                    mark.to_string(),
                    weight,
                    wt,
                    topic,
                    comment,
                ));
            }
        }

        if !dry_run {
            for (date, subj_name, mark, weight, wt, topic, comment) in pending {
                let subj = match self.find_subject(profile_id, &subj_name)?.exact {
                    Some(s) => s,
                    None => self.create_subject(
                        CreateSubjectInput {
                            profile_id,
                            name: subj_name,
                            short_name: None,
                            teacher: None,
                            color: None,
                            icon: None,
                            description: None,
                            academic_period_id: None,
                            target_average: None,
                        },
                        RecordSource::Import,
                        None,
                    )?,
                };
                let _ = self.add_grade(
                    AddGradeInput {
                        profile_id,
                        subject_id: subj.id,
                        date,
                        time: None,
                        mark_type_id: None,
                        mark_code: Some(mark),
                        numeric_grade: None,
                        weight: Some(weight),
                        work_type: Some(wt),
                        topic: Some(topic),
                        comment: Some(comment),
                        teacher: None,
                    },
                    RecordSource::Import,
                    None,
                );
            }
        }

        Ok(ImportPreview {
            format: "csv".into(),
            subjects_count: subjects_seen.len(),
            grades_count,
            attendance_count: 0,
            tasks_count: 0,
            warnings,
        })
    }

    /// Parses a user-selected Telegram Desktop `result.json` export purely for preview.
    /// Never connects to Telegram, never reads sessions/credentials, never auto-creates grades.
    pub fn preview_telegram_export(
        &self,
        path: impl AsRef<Path>,
    ) -> AppResult<TelegramImportPreview> {
        let raw = std::fs::read_to_string(path)?;
        let val: serde_json::Value = serde_json::from_str(&raw)?;
        let chat_name = val
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Telegram Export")
            .to_string();
        let messages = val
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::Import("JSON does not contain a 'messages' array".into()))?;

        let mut notes = Vec::new();
        for msg in messages {
            let id = msg.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let date = msg
                .get("date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = match msg.get("text") {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|p| {
                        p.as_str()
                            .map(String::from)
                            .or_else(|| p.get("text").and_then(|t| t.as_str()).map(String::from))
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                notes.push(TelegramMessagePreview {
                    id,
                    date,
                    text: trimmed.to_string(),
                });
            }
        }

        Ok(TelegramImportPreview {
            chat_name,
            total_messages: messages.len(),
            non_empty_notes: notes,
            notice: "Предпросмотр заметок из Telegram Desktop export. Автоматический разбор оценок не выполняется без явного подтверждения; для умного переноса используйте AI-ассистента через MCP.".into(),
        })
    }

    // ========================================================================
    // Consistent Online Backup Archive (ZIP) & Safe Restore
    // ========================================================================

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    pub fn create_backup_archive(&self, dest_zip: Option<PathBuf>) -> AppResult<BackupArchiveInfo> {
        let backups_dir = self.backups_dir();
        std::fs::create_dir_all(&backups_dir)?;
        let now = Utc::now();
        let zip_path = dest_zip.unwrap_or_else(|| {
            backups_dir.join(format!(
                "oxidium-backup-{}.zip",
                now.format("%Y%m%d-%H%M%S")
            ))
        });

        let temp_db = backups_dir.join(format!(".snapshot-{}.db", Uuid::new_v4()));
        self.db.backup_to_path(&temp_db)?;
        let db_bytes = std::fs::read(&temp_db)?;
        let _ = std::fs::remove_file(&temp_db);

        let db_sha = hex::encode(Sha256::digest(&db_bytes));
        let manifest = serde_json::json!({
            "app": "Oxidium",
            "version": env!("CARGO_PKG_VERSION"),
            "created_at": now.to_rfc3339(),
            "sqlite_version": journal_storage::Database::sqlite_version(),
            "database_sha256": db_sha,
        });

        let file = std::fs::File::create(&zip_path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("oxidium.db", options)
            .map_err(|e| AppError::Zip(e.to_string()))?;
        zip.write_all(&db_bytes)?;
        zip.start_file("manifest.json", options)
            .map_err(|e| AppError::Zip(e.to_string()))?;
        zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        zip.finish().map_err(|e| AppError::Zip(e.to_string()))?;

        let zip_bytes = std::fs::read(&zip_path)?;
        let zip_sha = hex::encode(Sha256::digest(&zip_bytes));
        self.rotate_old_backups()?;

        Ok(BackupArchiveInfo {
            path: zip_path,
            created_at: now,
            size_bytes: zip_bytes.len() as u64,
            sha256: zip_sha,
        })
    }

    pub fn restore_from_backup_archive(&self, zip_path: impl AsRef<Path>) -> AppResult<()> {
        let backups_dir = self.backups_dir();
        std::fs::create_dir_all(&backups_dir)?;

        let file = std::fs::File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::Backup(format!("invalid zip: {e}")))?;
        let mut db_entry = archive
            .by_name("oxidium.db")
            .map_err(|_| AppError::Backup("backup archive missing oxidium.db".into()))?;
        let mut buf = Vec::new();
        db_entry.read_to_end(&mut buf)?;

        let temp_restore = backups_dir.join(format!(".restore-{}.db", Uuid::new_v4()));
        std::fs::write(&temp_restore, &buf)?;
        let pre_safety = backups_dir.join(format!(
            "pre-restore-{}.db",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));

        let res = self.db.restore_from_path(&temp_restore, &pre_safety);
        let _ = std::fs::remove_file(&temp_restore);
        res?;
        self.invalidate_stats_cache();
        self.sync
            .notify_local_change(RecordSource::Import, "restore");
        Ok(())
    }

    pub fn rotate_old_backups(&self) -> AppResult<()> {
        let settings = self.db.load_settings()?;
        let keep = settings.auto_backup_keep_count.max(1) as usize;
        let dir = self.backups_dir();
        if !dir.exists() {
            return Ok(());
        }
        let mut zips: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("oxidium-backup-") && n.ends_with(".zip"))
            })
            .collect();
        zips.sort();
        while zips.len() > keep {
            if let Some(oldest) = zips.first() {
                let _ = std::fs::remove_file(oldest);
            }
            zips.remove(0);
        }
        Ok(())
    }
}

fn send_os_notification(title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = notify_rust::Notification::new()
            .appname("Oxidium")
            .summary(title)
            .body(body)
            .show();
    }
    #[cfg(target_os = "windows")]
    {
        use tauri_winrt_notification::Toast;
        let _ = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(body)
            .show();
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (title, body);
    }
}
