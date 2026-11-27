-- Oxidium Migration 0004: Remote E2EE Sync Config + Record-Level Tombstones for 3-Way SQLite Merge

CREATE TABLE IF NOT EXISTS sync_remote_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    auto_sync INTEGER NOT NULL DEFAULT 1 CHECK (auto_sync IN (0, 1)),
    server_url TEXT NOT NULL DEFAULT '',
    server_pub TEXT NOT NULL DEFAULT '',
    server_fingerprint TEXT NOT NULL DEFAULT '',
    device_id TEXT NOT NULL DEFAULT '',
    device_name TEXT NOT NULL DEFAULT '',
    vault_id TEXT NOT NULL DEFAULT '',
    last_synced_seq INTEGER NOT NULL DEFAULT 0,
    last_synced_local_rev INTEGER NOT NULL DEFAULT 0,
    last_sync_at TEXT,
    last_status TEXT NOT NULL DEFAULT 'disconnected',
    last_error TEXT,
    batch_debounce_secs INTEGER NOT NULL DEFAULT 8 CHECK (batch_debounce_secs >= 2),
    last_merge_summary TEXT
);

INSERT OR IGNORE INTO sync_remote_config (id) VALUES (1);

CREATE TABLE IF NOT EXISTS sync_tombstones (
    entity_table TEXT NOT NULL,
    entity_pk TEXT NOT NULL,
    deleted_at TEXT NOT NULL,
    PRIMARY KEY (entity_table, entity_pk)
);

CREATE INDEX IF NOT EXISTS idx_sync_tombstones_ts ON sync_tombstones(deleted_at);

-- Tombstone triggers on all 17 syncable entity/join tables (delete recording + undo-safe clear on re-insert)

CREATE TRIGGER IF NOT EXISTS tr_tombstone_profiles_del AFTER DELETE ON profiles
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('profiles', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_profiles_ins AFTER INSERT ON profiles
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'profiles' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_periods_del AFTER DELETE ON academic_periods
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('academic_periods', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_periods_ins AFTER INSERT ON academic_periods
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'academic_periods' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_subjects_del AFTER DELETE ON subjects
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('subjects', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_subjects_ins AFTER INSERT ON subjects
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'subjects' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_mark_types_del AFTER DELETE ON mark_types
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('mark_types', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_mark_types_ins AFTER INSERT ON mark_types
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'mark_types' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_grades_del AFTER DELETE ON grades
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('grades', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_grades_ins AFTER INSERT ON grades
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'grades' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_attendance_del AFTER DELETE ON attendance
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('attendance', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_attendance_ins AFTER INSERT ON attendance
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'attendance' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_boards_del AFTER DELETE ON boards
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('boards', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_boards_ins AFTER INSERT ON boards
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'boards' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_columns_del AFTER DELETE ON kanban_columns
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('kanban_columns', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_columns_ins AFTER INSERT ON kanban_columns
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'kanban_columns' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_tasks_del AFTER DELETE ON tasks
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('tasks', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_tasks_ins AFTER INSERT ON tasks
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'tasks' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_tags_del AFTER DELETE ON tags
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('tags', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_tags_ins AFTER INSERT ON tags
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'tags' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_badges_del AFTER DELETE ON badges
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('badges', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_badges_ins AFTER INSERT ON badges
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'badges' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_task_tags_del AFTER DELETE ON task_tags
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('task_tags', OLD.task_id || '|' || OLD.tag_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_task_tags_ins AFTER INSERT ON task_tags
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'task_tags' AND entity_pk = NEW.task_id || '|' || NEW.tag_id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_task_badges_del AFTER DELETE ON task_badges
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('task_badges', OLD.task_id || '|' || OLD.badge_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_task_badges_ins AFTER INSERT ON task_badges
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'task_badges' AND entity_pk = NEW.task_id || '|' || NEW.badge_id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_checklist_del AFTER DELETE ON task_checklist_items
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('task_checklist_items', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_checklist_ins AFTER INSERT ON task_checklist_items
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'task_checklist_items' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_grade_links_del AFTER DELETE ON task_grade_links
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('task_grade_links', OLD.task_id || '|' || OLD.grade_id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_grade_links_ins AFTER INSERT ON task_grade_links
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'task_grade_links' AND entity_pk = NEW.task_id || '|' || NEW.grade_id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_notifications_del AFTER DELETE ON notifications
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('notifications', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_notifications_ins AFTER INSERT ON notifications
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'notifications' AND entity_pk = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS tr_tombstone_reminders_del AFTER DELETE ON reminders
BEGIN
    INSERT INTO sync_tombstones(entity_table, entity_pk, deleted_at)
    VALUES ('reminders', OLD.id, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    ON CONFLICT(entity_table, entity_pk) DO UPDATE SET deleted_at = excluded.deleted_at;
END;
CREATE TRIGGER IF NOT EXISTS tr_tombstone_reminders_ins AFTER INSERT ON reminders
BEGIN
    DELETE FROM sync_tombstones WHERE entity_table = 'reminders' AND entity_pk = NEW.id;
END;
