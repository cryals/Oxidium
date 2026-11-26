-- Oxidium Migration 0002: Performance indexes and FTS5 global search index

CREATE INDEX IF NOT EXISTS idx_periods_profile ON academic_periods(profile_id, is_archived, start_date);
CREATE INDEX IF NOT EXISTS idx_subjects_profile_order ON subjects(profile_id, is_archived, sort_order);
CREATE INDEX IF NOT EXISTS idx_grades_profile_date ON grades(profile_id, date);
CREATE INDEX IF NOT EXISTS idx_grades_profile_subject_date ON grades(profile_id, subject_id, date);
CREATE INDEX IF NOT EXISTS idx_grades_subject_date ON grades(subject_id, date);
CREATE INDEX IF NOT EXISTS idx_attendance_profile_date ON attendance(profile_id, date);
CREATE INDEX IF NOT EXISTS idx_attendance_profile_subject_date ON attendance(profile_id, subject_id, date);
CREATE INDEX IF NOT EXISTS idx_boards_profile ON boards(profile_id, is_archived, sort_order);
CREATE INDEX IF NOT EXISTS idx_columns_board_order ON kanban_columns(board_id, is_archived, sort_order);
CREATE INDEX IF NOT EXISTS idx_tasks_board_col_order ON tasks(board_id, column_id, is_archived, sort_order);
CREATE INDEX IF NOT EXISTS idx_tasks_profile_due ON tasks(profile_id, is_archived, due_at);
CREATE INDEX IF NOT EXISTS idx_tasks_profile_subject ON tasks(profile_id, subject_id, is_archived);
CREATE INDEX IF NOT EXISTS idx_checklist_task_order ON task_checklist_items(task_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders(is_fired, remind_at);
CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications(is_dismissed, is_read, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_activity_log_ts ON activity_log(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_activity_log_profile ON activity_log(profile_id, timestamp DESC);

-- FTS5 global search index supporting Unicode / Cyrillic tokens
CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    entity_type UNINDEXED,
    entity_id UNINDEXED,
    profile_id UNINDEXED,
    subject_id UNINDEXED,
    entity_date UNINDEXED,
    title,
    body,
    tokenize='unicode61 remove_diacritics 0'
);
