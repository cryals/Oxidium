# Oxidium MCP Tools Reference (80 Tools)

All write tools enforce explicit `profile_id` isolation, server-side permissions (`ReadOnly` / `ReadWrite` / `Destructive`), ISO 8601 dates (`YYYY-MM-DD` / RFC 3339), and optional `idempotency_key` deduplication.

## Profiles (6)
- `list_profiles` (Read) — list active or archived profiles.
- `get_profile` (Read) — fetch a profile by UUID.
- `get_active_profile` (Read) — fetch the currently selected profile.
- `create_profile` (Write) — create an isolated profile with default Kanban board.
- `update_profile` (Write) — edit profile metadata.
- `archive_profile` (Write) — archive or restore a profile.

## Subjects (7)
- `list_subjects` (Read) — list subjects in a profile.
- `get_subject` (Read) — fetch a subject by UUID.
- `find_subject` (Read) — find by name/short name; returns `ambiguous: true` with `candidates` when multiple subjects match.
- `create_subject` (Write) — create a subject in an explicit profile.
- `update_subject` (Write) — update subject metadata or target average.
- `archive_subject` (Write) — archive or restore (preserves historical grades).
- `reorder_subjects` (Write) — reorder subjects within a profile.

## Grades (12)
- `list_grades` (Read) — filtered, paginated grade listing.
- `get_grade` (Read) — single grade by UUID.
- `add_grade` (Write, idempotent) — add numeric grade or pass/fail/custom mark.
- `add_grades_batch` (Write, atomic, idempotent) — batch insert up to 200 grades in one transaction.
- `update_grade` (Write) — edit date, weight, work type, topic, comment, teacher.
- `delete_grade` (Destructive) — delete a grade (blocked unless Destructive Actions are enabled).
- `get_subject_grades` (Read) — all grades for a subject.
- `get_grades_by_date` (Read) — all grades on an ISO date.
- `get_grades_by_period` (Read) — all grades in an inclusive date range.
- `get_average_grade` (Read) — exact arithmetic or weighted average (`Decimal`).
- `get_grade_statistics` (Read) — distribution & average summary.
- `calculate_grade_forecast` (Read) — simulate hypothetical grades or compute exact grades needed to reach a target GPA (never writes to DB).

## Attendance (5)
- `add_attendance` (Write, idempotent) — record `absent`, `excused`, `present`, or `custom:<code>` independently of numeric grades.
- `update_attendance` (Write) — edit an attendance entry.
- `delete_attendance` (Destructive) — delete an attendance entry.
- `list_attendance` (Read) — list attendance records by profile/subject/period.
- `get_attendance_statistics` (Read) — honest absence counts (never fabricates attendance % from empty days).

## Journal (7)
- `get_journal` (Read) — structured journal matrix for a date range.
- `get_monthly_journal` (Read) — full monthly matrix.
- `get_journal_summary` (Read) — compact period summary.
- `search_journal` (Read) — FTS5 + Unicode search across subjects, grades, attendance, tasks, tags, badges.
- `get_daily_summary` (Read) — single-day summary.
- `get_weekly_summary` (Read) — week summary honoring configured week start.
- `get_monthly_summary` (Read) — monthly academic summary.

## Kanban Boards & Columns (9)
- `list_boards`, `get_board`, `create_board`, `update_board`
- `list_columns`, `create_column`, `update_column`, `move_column`, `archive_column`

## Tasks (14)
- `list_tasks`, `get_task`, `create_task`, `create_tasks_batch` (atomic, idempotent), `update_task`, `move_task`, `complete_task`, `reopen_task`, `archive_task`, `delete_task` (Destructive), `get_overdue_tasks`, `get_upcoming_tasks`, `get_tasks_by_subject`, `search_tasks`

## Tags, Badges & Checklists (12)
- `list_tags`, `create_tag` (case-insensitive dedup), `update_tag`, `assign_tag`, `remove_tag`
- `list_badges`, `create_badge` (case-insensitive dedup), `assign_badge`
- `add_checklist_item`, `update_checklist_item`, `complete_checklist_item`, `delete_checklist_item` (Destructive)

## Statistics (8 + shared forecast)
- `get_overall_statistics`, `get_subject_statistics`, `get_grade_distribution`, `get_average_history`, `get_attendance_summary`, `get_task_statistics`, `compare_periods`, `get_subjects_below_target` (plus `calculate_grade_forecast`).
