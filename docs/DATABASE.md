# Oxidium Database Architecture

Oxidium stores all user data locally in a single SQLite database located via `directories::ProjectDirs::from("io", "oxidium", "Oxidium")` (or overridden via `--db <path>` / `OXIDIUM_DB_PATH`).

## SQLite Configuration
- `PRAGMA journal_mode = WAL;` — concurrent reads + safe multi-process GUI/MCP access.
- `PRAGMA synchronous = NORMAL;` — crash-safe with WAL and high write throughput.
- `PRAGMA foreign_keys = ON;` — strict referential integrity enforced on every connection.
- `PRAGMA busy_timeout = 5000;` — eliminates transient lock failures under simultaneous GUI/MCP writes.

## Tables (20 Core + Support Tables)
1. `profiles` — isolated academic profiles (`id`, `name`, `icon`, `color`, `institution`, `group_name`, `academic_year`, `description`, `is_archived`, timestamps).
2. `academic_periods` — flexible periods (`year`, `semester`, `quarter`, `trimester`, `custom`) scoped per profile.
3. `subjects` — subjects scoped per profile (`sort_order`, `target_average`, `is_archived`, optional `academic_period_id`).
4. `mark_types` — built-in (`1..5`, `Н/Б`, `Уваж.`, `Зач.`, `Н/З`, `Прис.`) and user-defined mark types with strict `category` (`numeric_grade`, `attendance`, `pass_fail`, `custom_status`), `counts_in_average`, and optional `numeric_value`.
5. `grades` — individual grade/mark events (`weight > 0`, `work_type`, `topic`, `comment`, `teacher`, `source IN ('gui','mcp','import')`). Supports multiple grades per subject per day.
6. `attendance` — independent attendance events (`absent`, `excused`, `present`, `custom`), never conflated with numeric grades.
7. `boards` — multiple Kanban boards per profile.
8. `kanban_columns` — dynamic columns per board (`wip_limit`, `is_completed_column`, `is_collapsed`, `is_archived`, `sort_order`).
9. `tasks` — Kanban task cards (`priority`, `color`, `start_date`, `due_at`, `completed_at`, `description`, `notes`, `sort_order`, `is_archived`).
10. `tags` — reusable profile-scoped tags with case-insensitive uniqueness.
11. `badges` — reusable profile-scoped custom badges.
12. `task_tags` — M:N link (`ON DELETE CASCADE`).
13. `task_badges` — M:N link (`ON DELETE CASCADE`).
14. `task_checklist_items` — ordered checklist items per task (`ON DELETE CASCADE`).
15. `task_grade_links` — links completed tasks to journal grades (`ON DELETE CASCADE`).
16. `notifications` — internal notification center entries (`read`, `dismissed`, `kind`, `entity_id`).
17. `reminders` — scheduled task deadline reminders with custom/preset intervals.
18. `settings` — typed key-value persistent configuration (excluding MCP Bearer token secret!).
19. `activity_log` — auditable mutation history (GUI / MCP / import) with correlation IDs and JSON undo payloads (secrets strictly excluded).
20. `schema_migrations` — versioned migration ledger with SHA-256 checksums.
21. Support tables: `search_index` (FTS5 virtual table with `unicode61` tokenizer), `sync_state` (monotonic cross-process revision counter), `mcp_idempotency_keys` (deduplication store for retried MCP writes), `holidays` (configurable holiday highlights).

## Referential Safety & Backups
- User-data foreign keys use `ON DELETE RESTRICT` to prevent uncontrolled cascade deletion of grades or tasks; deliberate destructive actions explicitly validate and delete inside a single transaction.
- Backups use SQLite's native online Backup API (`rusqlite::backup::Backup`), safe even while WAL transactions are active.
- Restore runs `PRAGMA integrity_check` on the candidate snapshot, automatically creates a pre-restore backup of the current database, and rolls back automatically if restore fails.
