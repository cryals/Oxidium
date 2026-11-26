use chrono::{NaiveDate, Utc};
use journal_domain::{
    AttendanceEntry, AttendanceStatus, Board, ChecklistItem, GradeEntry, KanbanColumn,
    MarkCategory, Profile, RecordSource, SearchEntityType, Subject, Tag, Task, TaskPriority,
    WorkType, builtin_marks,
};
use journal_storage::{Database, SearchFilter};
use rust_decimal::Decimal;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

fn make_profile(name: &str) -> Profile {
    let now = Utc::now();
    Profile {
        id: Uuid::new_v4(),
        name: name.into(),
        icon: "school".into(),
        color: "#445E91".into(),
        institution: "МГУ".into(),
        group_name: "ИВТ-21".into(),
        academic_year: "2026-2027".into(),
        description: "Основной учебный профиль".into(),
        is_archived: false,
        created_at: now,
        updated_at: now,
    }
}

fn make_subject(profile_id: Uuid, name: &str) -> Subject {
    let now = Utc::now();
    Subject {
        id: Uuid::new_v4(),
        profile_id,
        academic_period_id: None,
        name: name.into(),
        short_name: name.chars().take(4).collect(),
        teacher: "Иванов И.И.".into(),
        color: "#2E7D32".into(),
        icon: "book".into(),
        description: "Лекции и семинары".into(),
        sort_order: 0,
        is_archived: false,
        target_average: Some(Decimal::new(45, 1)),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn migrations_are_idempotent_and_seed_builtins() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("oxidium.db");
    {
        let db = Database::open(&db_path).unwrap();
        assert!(db.check_integrity().unwrap());
        assert_eq!(db.applied_migrations().unwrap().len(), 3);
        let marks = db.list_mark_types(None).unwrap();
        assert_eq!(marks.len(), 10);
    }
    // Re-open existing DB: migrations stay applied with zero error
    let db2 = Database::open(&db_path).unwrap();
    assert_eq!(db2.applied_migrations().unwrap().len(), 3);
    assert_eq!(db2.list_mark_types(None).unwrap().len(), 10);
}

#[test]
fn fts5_russian_search_and_profile_isolation() {
    let db = Database::open_in_memory().unwrap();
    let p1 = make_profile("Университет");
    let p2 = make_profile("Онлайн-курсы");
    db.insert_profile(&p1, RecordSource::Gui).unwrap();
    db.insert_profile(&p2, RecordSource::Gui).unwrap();

    let s1 = make_subject(p1.id, "Базы данных");
    let s2 = make_subject(p2.id, "Философия");
    db.insert_subject(&s1, RecordSource::Gui).unwrap();
    db.insert_subject(&s2, RecordSource::Gui).unwrap();

    let now = Utc::now();
    let grade = GradeEntry {
        id: Uuid::new_v4(),
        profile_id: p1.id,
        subject_id: s1.id,
        date: NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
        time: None,
        mark_type_id: builtin_marks::GRADE_5_ID,
        mark_code: "grade_5".into(),
        mark_label: "5".into(),
        mark_category: MarkCategory::NumericGrade,
        counts_in_average: true,
        numeric_value: Some(Decimal::from(5)),
        weight: Decimal::from(2),
        work_type: WorkType::LabWork,
        topic: "Нормализация отношений".into(),
        comment: "Отличная защита лабораторной".into(),
        teacher: "Петров П.П.".into(),
        source: RecordSource::Gui,
        created_at: now,
        updated_at: now,
    };
    db.insert_grade(&grade).unwrap();

    // Search Russian text scoped to profile 1
    let res = db
        .search_global(
            "Нормализ",
            &SearchFilter {
                profile_id: Some(p1.id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].entity_type, SearchEntityType::Grade);
    assert_eq!(res[0].entity_id, grade.id);

    // Must not leak into profile 2
    let res_p2 = db
        .search_global(
            "Нормализ",
            &SearchFilter {
                profile_id: Some(p2.id),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(res_p2.is_empty());

    // Case-insensitive Cyrillic mid-word fallback
    let fallback = db
        .search_global(
            "философ",
            &SearchFilter {
                profile_id: Some(p2.id),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(fallback.len(), 1);
    assert_eq!(fallback[0].entity_id, s2.id);
}

#[test]
fn kanban_hydration_and_attendance_independence() {
    let db = Database::open_in_memory().unwrap();
    let p = make_profile("Колледж");
    db.insert_profile(&p, RecordSource::Gui).unwrap();
    let s = make_subject(p.id, "Программирование");
    db.insert_subject(&s, RecordSource::Gui).unwrap();

    let now = Utc::now();
    let att = AttendanceEntry {
        id: Uuid::new_v4(),
        profile_id: p.id,
        subject_id: s.id,
        date: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
        time: None,
        status: AttendanceStatus::Excused,
        mark_type_id: builtin_marks::EXCUSED_ID,
        reason: "Справка от врача".into(),
        comment: String::new(),
        source: RecordSource::Mcp,
        created_at: now,
        updated_at: now,
    };
    db.insert_attendance(&att).unwrap();
    assert_eq!(db.list_attendance(p.id, None, None, None).unwrap().len(), 1);
    assert!(
        db.list_grades(p.id, None, None, None, None, None, None, None)
            .unwrap()
            .is_empty()
    );

    let board = Board {
        id: Uuid::new_v4(),
        profile_id: p.id,
        name: "Основная".into(),
        description: String::new(),
        color: "#445E91".into(),
        sort_order: 0,
        is_archived: false,
        created_at: now,
        updated_at: now,
    };
    db.insert_board(&board, RecordSource::Gui).unwrap();
    let col = KanbanColumn {
        id: Uuid::new_v4(),
        board_id: board.id,
        name: "В процессе".into(),
        color: "#2563EB".into(),
        sort_order: 0,
        is_collapsed: false,
        is_archived: false,
        wip_limit: Some(5),
        is_completed_column: false,
        created_at: now,
        updated_at: now,
    };
    db.insert_column(&col, RecordSource::Gui).unwrap();

    let tag = Tag {
        id: Uuid::new_v4(),
        profile_id: p.id,
        name: "Лабораторная".into(),
        color: "#16A34A".into(),
        icon: None,
        description: String::new(),
    };
    db.insert_tag(&tag, RecordSource::Gui).unwrap();
    assert!(
        db.find_tag_by_name(p.id, "  лабораторная ")
            .unwrap()
            .is_some()
    );

    let task = Task {
        id: Uuid::new_v4(),
        profile_id: p.id,
        board_id: board.id,
        column_id: col.id,
        subject_id: Some(s.id),
        title: "Лабораторная №4".into(),
        description: "Реализовать B-tree".into(),
        priority: TaskPriority::High,
        color: None,
        start_date: None,
        due_at: None,
        completed_at: None,
        notes: String::new(),
        sort_order: 0,
        is_archived: false,
        created_at: now,
        updated_at: now,
        tags: vec![tag],
        badges: vec![],
        checklist: vec![ChecklistItem {
            id: Uuid::new_v4(),
            task_id: Uuid::nil(),
            title: "Написать тесты".into(),
            is_completed: false,
            sort_order: 0,
            created_at: now,
            updated_at: now,
        }],
        linked_grade_ids: vec![],
    };
    db.insert_task(&task, RecordSource::Gui).unwrap();
    let loaded = db.get_task(task.id).unwrap().unwrap();
    assert_eq!(loaded.tags.len(), 1);
    assert_eq!(loaded.checklist.len(), 1);
}

#[test]
fn online_backup_restore_and_corrupt_rejection() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("live.db");
    let backup_path = dir.path().join("backup.db");
    let safety_path = dir.path().join("pre_restore.db");
    let corrupt_path = dir.path().join("corrupt.db");

    let db = Database::open(&db_path).unwrap();
    let p1 = make_profile("Исходный профиль");
    db.insert_profile(&p1, RecordSource::Gui).unwrap();
    db.backup_to_path(&backup_path).unwrap();

    // Mutate after backup
    let p2 = make_profile("Новый профиль после бэкапа");
    db.insert_profile(&p2, RecordSource::Gui).unwrap();
    assert_eq!(db.list_profiles(true).unwrap().len(), 2);

    // Reject corrupt file and preserve live data
    std::fs::write(&corrupt_path, b"not a valid sqlite file").unwrap();
    assert!(db.restore_from_path(&corrupt_path, &safety_path).is_err());
    assert_eq!(db.list_profiles(true).unwrap().len(), 2);

    // Restore valid backup
    db.restore_from_path(&backup_path, &safety_path).unwrap();
    let restored = db.list_profiles(true).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, p1.id);
    assert!(safety_path.exists());
}

#[test]
fn concurrent_wal_multi_connection_writes() {
    let dir = TempDir::new().unwrap();
    let db_path = Arc::new(dir.path().join("concurrent.db"));
    let db = Database::open(&*db_path).unwrap();
    let p = make_profile("Параллельный");
    db.insert_profile(&p, RecordSource::Gui).unwrap();
    let s = make_subject(p.id, "Математика");
    db.insert_subject(&s, RecordSource::Gui).unwrap();

    let mut handles = Vec::new();
    for worker in 0..4 {
        let path = Arc::clone(&db_path);
        let pid = p.id;
        let sid = s.id;
        handles.push(std::thread::spawn(move || {
            let conn = Database::open(&*path).unwrap();
            for i in 0..25 {
                let now = Utc::now();
                let g = GradeEntry {
                    id: Uuid::new_v4(),
                    profile_id: pid,
                    subject_id: sid,
                    date: NaiveDate::from_ymd_opt(2026, 11, ((i % 28) + 1) as u32).unwrap(),
                    time: None,
                    mark_type_id: builtin_marks::GRADE_5_ID,
                    mark_code: "grade_5".into(),
                    mark_label: "5".into(),
                    mark_category: MarkCategory::NumericGrade,
                    counts_in_average: true,
                    numeric_value: Some(Decimal::from(5)),
                    weight: Decimal::ONE,
                    work_type: WorkType::ClassAnswer,
                    topic: format!("W{worker}-{i}"),
                    comment: String::new(),
                    teacher: String::new(),
                    source: if worker % 2 == 0 {
                        RecordSource::Gui
                    } else {
                        RecordSource::Mcp
                    },
                    created_at: now,
                    updated_at: now,
                };
                conn.insert_grade(&g).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let all = db
        .list_grades(p.id, None, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(all.len(), 100);
    assert!(db.get_sync_state().unwrap().revision >= 102);
}
