use chrono::{Duration, NaiveDate, Utc};
use journal_app::{AppServices, CreateProfileInput, CreateSubjectInput, StatisticsFilter};
use journal_domain::{
    GradeEntry, MarkCategory, RecordSource, Task, TaskPriority, WorkType, builtin_marks,
};
use journal_storage::SearchFilter;
use rust_decimal::Decimal;
use std::time::Instant;
use tempfile::tempdir;
use uuid::Uuid;

fn make_grade(
    profile_id: Uuid,
    subject_id: Uuid,
    date: NaiveDate,
    val: u8,
    idx: usize,
) -> GradeEntry {
    let now = Utc::now();
    let mark_type_id = builtin_marks::grade_id_for_value(val).expect("grade 1..5");
    GradeEntry {
        id: Uuid::new_v4(),
        profile_id,
        subject_id,
        date,
        time: None,
        mark_type_id,
        mark_code: format!("grade_{val}"),
        mark_label: val.to_string(),
        mark_category: MarkCategory::NumericGrade,
        counts_in_average: true,
        numeric_value: Some(Decimal::from(val)),
        weight: Decimal::ONE,
        work_type: WorkType::ClassAnswer,
        topic: if idx.is_multiple_of(20) {
            format!("Контрольная тема #{idx}")
        } else {
            String::new()
        },
        comment: String::new(),
        teacher: String::new(),
        source: RecordSource::Import,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn benchmark_100_subjects_10k_grades_and_multi_profile() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("bench-10k.db");

    let t_open = Instant::now();
    let app = AppServices::open_file(&db_path).expect("open_file");
    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;

    let p1 = app.get_active_profile().expect("active profile");
    let p2 = app
        .create_profile(
            CreateProfileInput {
                name: "Второй профиль".into(),
                icon: None,
                color: Some("#0F766E".into()),
                institution: Some("Университет".into()),
                group_name: Some("Б2".into()),
                academic_year: Some("2026/2027".into()),
                description: None,
            },
            RecordSource::Gui,
            None,
        )
        .expect("profile 2");

    // 100 subjects in profile 1
    let t_subj = Instant::now();
    let mut subject_ids = Vec::with_capacity(100);
    for i in 0..100 {
        let s = app
            .create_subject(
                CreateSubjectInput {
                    profile_id: p1.id,
                    name: format!("Предмет #{:03}", i + 1),
                    short_name: Some(format!("П{:02}", i + 1)),
                    teacher: Some(format!("Преподаватель {}", i + 1)),
                    color: Some("#4A4459".into()),
                    icon: None,
                    description: None,
                    academic_period_id: None,
                    target_average: Some(Decimal::new(45, 1)),
                },
                RecordSource::Gui,
                None,
            )
            .expect("create subject");
        subject_ids.push(s.id);
    }
    let subj_ms = t_subj.elapsed().as_secs_f64() * 1000.0;

    // 10,000 grades distributed across 100 subjects and 2026 calendar
    let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let grades: Vec<GradeEntry> = (0..10_000)
        .map(|i| {
            let sid = subject_ids[i % 100];
            let date = base + Duration::days((i % 360) as i64);
            let val = ((i % 5) + 1) as u8;
            make_grade(p1.id, sid, date, val, i)
        })
        .collect();

    let t_insert = Instant::now();
    app.db
        .insert_grades_batch(&grades, RecordSource::Import)
        .expect("insert 10k grades");
    let insert_ms = t_insert.elapsed().as_secs_f64() * 1000.0;

    // Monthly journal matrix query (100 subjects x 30 days)
    let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
    let t_matrix = Instant::now();
    let matrix = app
        .get_monthly_journal(p1.id, 2026, 11, today)
        .expect("monthly journal");
    let matrix_ms = t_matrix.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(matrix.rows.len(), 100);
    assert_eq!(matrix.days.len(), 30);

    // Statistics cold & cached
    let filter = StatisticsFilter {
        profile_id: p1.id,
        ..Default::default()
    };
    let t_stats_cold = Instant::now();
    let stats = app.get_statistics(&filter).expect("stats cold");
    let stats_cold_ms = t_stats_cold.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(stats.numeric_grade_count, 10_000);

    let t_stats_cached = Instant::now();
    let stats2 = app.get_statistics(&filter).expect("stats cached");
    let stats_cached_ms = t_stats_cached.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(stats2.numeric_grade_count, 10_000);

    // FTS5 search
    let s_filter = SearchFilter {
        profile_id: Some(p1.id),
        limit: 25,
        ..Default::default()
    };
    let t_search = Instant::now();
    let hits = app.search_global("Контрольная", &s_filter).expect("search");
    let search_ms = t_search.elapsed().as_secs_f64() * 1000.0;
    assert!(!hits.is_empty());

    // Multi-profile isolation check
    let p2_stats = app
        .get_statistics(&StatisticsFilter {
            profile_id: p2.id,
            ..Default::default()
        })
        .expect("p2 stats");
    assert_eq!(p2_stats.numeric_grade_count, 0);

    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "[BENCH-10K] open={open_ms:.2}ms | 100_subjects={subj_ms:.2}ms | insert_10k_grades={insert_ms:.2}ms | monthly_matrix_100x30={matrix_ms:.2}ms | stats_cold={stats_cold_ms:.2}ms | stats_cached={stats_cached_ms:.2}ms | fts5_search={search_ms:.2}ms | db_kb={:.1}",
        db_bytes as f64 / 1024.0
    );
}

#[test]
#[ignore = "Heavy scale benchmark: run with `cargo test --release -p journal-app --test perf_benchmarks -- --ignored --nocapture`"]
fn benchmark_scale_100k_grades_and_10k_tasks() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("bench-100k.db");

    let t_open = Instant::now();
    let app = AppServices::open_file(&db_path).expect("open_file");
    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;

    let p = app.get_active_profile().expect("active profile");
    let mut subject_ids = Vec::with_capacity(100);
    for i in 0..100 {
        let s = app
            .create_subject(
                CreateSubjectInput {
                    profile_id: p.id,
                    name: format!("Дисциплина #{:03}", i + 1),
                    short_name: Some(format!("Д{:02}", i + 1)),
                    teacher: Some(format!("Лектор {}", i + 1)),
                    color: Some("#4A4459".into()),
                    icon: None,
                    description: None,
                    academic_period_id: None,
                    target_average: Some(Decimal::new(45, 1)),
                },
                RecordSource::Gui,
                None,
            )
            .expect("subject");
        subject_ids.push(s.id);
    }

    // Insert 100,000 grades in 10 batches of 10,000
    let base = NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();
    let t_grades = Instant::now();
    for chunk in 0..10 {
        let batch: Vec<GradeEntry> = (0..10_000)
            .map(|j| {
                let i = chunk * 10_000 + j;
                let sid = subject_ids[i % 100];
                let date = base + Duration::days((i % 1800) as i64);
                let val = ((i % 5) + 1) as u8;
                make_grade(p.id, sid, date, val, i)
            })
            .collect();
        app.db
            .insert_grades_batch(&batch, RecordSource::Import)
            .expect("batch grades");
    }
    let grades_100k_ms = t_grades.elapsed().as_secs_f64() * 1000.0;

    // Insert 10,000 Kanban tasks across 4 columns
    let boards = app.list_boards(p.id, false).expect("boards");
    let cols = app.list_columns(boards[0].id, false).expect("cols");
    let now = Utc::now();
    let tasks: Vec<Task> = (0..10_000)
        .map(|i| {
            let col_idx = i % cols.len();
            Task {
                id: Uuid::new_v4(),
                profile_id: p.id,
                board_id: boards[0].id,
                column_id: cols[col_idx].id,
                subject_id: Some(subject_ids[i % 100]),
                title: format!("Учебная задача #{i} по модулю {}", i % 50),
                description: format!("Описание лабораторной работы #{i}"),
                priority: match i % 4 {
                    0 => TaskPriority::Low,
                    1 => TaskPriority::Medium,
                    2 => TaskPriority::High,
                    _ => TaskPriority::Critical,
                },
                color: None,
                start_date: None,
                due_at: Some(now + Duration::days((i as i64 % 90) - 30)),
                completed_at: if cols[col_idx].is_completed_column {
                    Some(now)
                } else {
                    None
                },
                notes: String::new(),
                sort_order: i as i32,
                is_archived: false,
                created_at: now,
                updated_at: now,
                tags: vec![],
                badges: vec![],
                checklist: vec![],
                linked_grade_ids: vec![],
            }
        })
        .collect();
    let t_tasks = Instant::now();
    app.db
        .insert_tasks_batch(&tasks, RecordSource::Import)
        .expect("insert 10k tasks");
    let tasks_10k_ms = t_tasks.elapsed().as_secs_f64() * 1000.0;

    // Monthly journal matrix on 100k-grade DB
    let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
    let t_matrix = Instant::now();
    let matrix = app
        .get_monthly_journal(p.id, 2026, 11, today)
        .expect("monthly journal");
    let matrix_ms = t_matrix.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(matrix.rows.len(), 100);

    // Full 100,000-grade statistics (cold + cached)
    let filter = StatisticsFilter {
        profile_id: p.id,
        ..Default::default()
    };
    let t_stats_cold = Instant::now();
    let stats = app.get_statistics(&filter).expect("100k stats cold");
    let stats_cold_ms = t_stats_cold.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(stats.numeric_grade_count, 100_000);

    let t_stats_cached = Instant::now();
    let stats2 = app.get_statistics(&filter).expect("100k stats cached");
    let stats_cached_ms = t_stats_cached.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(stats2.numeric_grade_count, 100_000);

    // Load 10,000 Kanban tasks
    let t_kanban = Instant::now();
    let loaded_tasks = app
        .list_tasks(p.id, Some(boards[0].id), None, None, false)
        .expect("list 10k tasks");
    let kanban_ms = t_kanban.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(loaded_tasks.len(), 10_000);

    // FTS5 search across 100k grades + 10k tasks
    let s_filter = SearchFilter {
        profile_id: Some(p.id),
        limit: 30,
        ..Default::default()
    };
    let t_search = Instant::now();
    let hits = app
        .search_global("лабораторной", &s_filter)
        .expect("fts5 100k");
    let search_ms = t_search.elapsed().as_secs_f64() * 1000.0;
    assert!(!hits.is_empty());

    // Online DB reopen time
    drop(app);
    let t_reopen = Instant::now();
    let _app2 = AppServices::open_file(&db_path).expect("reopen 100k db");
    let reopen_ms = t_reopen.elapsed().as_secs_f64() * 1000.0;

    let db_mb = std::fs::metadata(&db_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    println!(
        "[BENCH-100K] open_empty={open_ms:.2}ms | reopen_100k={reopen_ms:.2}ms | insert_100k_grades={grades_100k_ms:.1}ms | insert_10k_tasks={tasks_10k_ms:.1}ms | monthly_matrix_100x30={matrix_ms:.2}ms | stats_100k_cold={stats_cold_ms:.2}ms | stats_100k_cached={stats_cached_ms:.3}ms | list_10k_tasks={kanban_ms:.2}ms | fts5_search={search_ms:.2}ms | db_size={db_mb:.2}MB"
    );
}
