-- Oxidium Migration 0001: Initial core relational schema

CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    icon TEXT NOT NULL DEFAULT 'school',
    color TEXT NOT NULL DEFAULT '#445E91',
    institution TEXT NOT NULL DEFAULT '',
    group_name TEXT NOT NULL DEFAULT '',
    academic_year TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS academic_periods (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    period_type TEXT NOT NULL CHECK (period_type IN ('academic_year', 'semester', 'quarter', 'trimester', 'custom')),
    start_date TEXT NOT NULL,
    end_date TEXT NOT NULL,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (start_date <= end_date)
);

CREATE TABLE IF NOT EXISTS subjects (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    academic_period_id TEXT REFERENCES academic_periods(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    short_name TEXT NOT NULL DEFAULT '',
    teacher TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '#445E91',
    icon TEXT NOT NULL DEFAULT 'book',
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    target_average TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mark_types (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT REFERENCES profiles(id) ON DELETE RESTRICT,
    code TEXT NOT NULL CHECK (length(trim(code)) > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    short_label TEXT NOT NULL CHECK (length(trim(short_label)) > 0),
    category TEXT NOT NULL CHECK (category IN ('numeric_grade', 'attendance', 'pass_fail', 'custom_status')),
    color TEXT NOT NULL DEFAULT '#445E91',
    icon TEXT,
    counts_in_average INTEGER NOT NULL DEFAULT 0 CHECK (counts_in_average IN (0, 1)),
    numeric_value TEXT,
    is_builtin INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_mark_types_builtin_code
    ON mark_types(code) WHERE profile_id IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS ux_mark_types_profile_code
    ON mark_types(profile_id, code) WHERE profile_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS grades (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    subject_id TEXT NOT NULL REFERENCES subjects(id) ON DELETE RESTRICT,
    date TEXT NOT NULL,
    time TEXT,
    mark_type_id TEXT NOT NULL REFERENCES mark_types(id) ON DELETE RESTRICT,
    numeric_value TEXT,
    weight TEXT NOT NULL DEFAULT '1',
    work_type TEXT NOT NULL DEFAULT 'class_answer' CHECK (work_type IN (
        'class_answer', 'homework', 'control_work', 'independent_work',
        'lab_work', 'practical_work', 'test', 'project', 'exam', 'credit', 'other'
    )),
    topic TEXT NOT NULL DEFAULT '',
    comment TEXT NOT NULL DEFAULT '',
    teacher TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'gui' CHECK (source IN ('gui', 'mcp', 'import')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (CAST(weight AS REAL) > 0.0)
);

CREATE TABLE IF NOT EXISTS attendance (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    subject_id TEXT NOT NULL REFERENCES subjects(id) ON DELETE RESTRICT,
    date TEXT NOT NULL,
    time TEXT,
    status TEXT NOT NULL CHECK (length(trim(status)) > 0),
    mark_type_id TEXT NOT NULL REFERENCES mark_types(id) ON DELETE RESTRICT,
    reason TEXT NOT NULL DEFAULT '',
    comment TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'gui' CHECK (source IN ('gui', 'mcp', 'import')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS boards (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    description TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL DEFAULT '#445E91',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS kanban_columns (
    id TEXT PRIMARY KEY NOT NULL,
    board_id TEXT NOT NULL REFERENCES boards(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    color TEXT NOT NULL DEFAULT '#64748B',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_collapsed INTEGER NOT NULL DEFAULT 0 CHECK (is_collapsed IN (0, 1)),
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    wip_limit INTEGER CHECK (wip_limit IS NULL OR wip_limit > 0),
    is_completed_column INTEGER NOT NULL DEFAULT 0 CHECK (is_completed_column IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    board_id TEXT NOT NULL REFERENCES boards(id) ON DELETE RESTRICT,
    column_id TEXT NOT NULL REFERENCES kanban_columns(id) ON DELETE RESTRICT,
    subject_id TEXT REFERENCES subjects(id) ON DELETE SET NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    priority INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 4),
    color TEXT,
    start_date TEXT,
    due_at TEXT,
    completed_at TEXT,
    notes TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    name_normalized TEXT NOT NULL CHECK (length(trim(name_normalized)) > 0),
    color TEXT NOT NULL DEFAULT '#445E91',
    icon TEXT,
    description TEXT NOT NULL DEFAULT '',
    UNIQUE (profile_id, name_normalized)
);

CREATE TABLE IF NOT EXISTS badges (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    name_normalized TEXT NOT NULL CHECK (length(trim(name_normalized)) > 0),
    color TEXT NOT NULL DEFAULT '#7C3AED',
    icon TEXT,
    description TEXT NOT NULL DEFAULT '',
    UNIQUE (profile_id, name_normalized)
);

CREATE TABLE IF NOT EXISTS task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);

CREATE TABLE IF NOT EXISTS task_badges (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    badge_id TEXT NOT NULL REFERENCES badges(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, badge_id)
);

CREATE TABLE IF NOT EXISTS task_checklist_items (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    is_completed INTEGER NOT NULL DEFAULT 0 CHECK (is_completed IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_grade_links (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    grade_id TEXT NOT NULL REFERENCES grades(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (task_id, grade_id)
);

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT REFERENCES profiles(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    entity_type TEXT,
    entity_id TEXT,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    is_dismissed INTEGER NOT NULL DEFAULT 0 CHECK (is_dismissed IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reminders (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    remind_at TEXT NOT NULL,
    offset_minutes INTEGER NOT NULL DEFAULT 0 CHECK (offset_minutes >= 0),
    is_fired INTEGER NOT NULL DEFAULT 0 CHECK (is_fired IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS activity_log (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp TEXT NOT NULL,
    client_identity TEXT,
    tool_name TEXT,
    profile_id TEXT,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    operation_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    result_status TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('gui', 'mcp', 'import')),
    correlation_id TEXT NOT NULL,
    undo_payload_json TEXT,
    undone INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0, 1))
);
