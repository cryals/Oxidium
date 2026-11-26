use chrono::{Duration, NaiveDate, Utc};
use journal_app::{
    AddAttendanceInput, AddGradeInput, AppServices, AuditMeta, CreateProfileInput,
    CreateSubjectInput, CreateTaskInput, StatisticsFilter, i18n, redact_secrets,
};
use journal_domain::{
    AttendanceStatus, AverageMode, GradeSample, Language, RecordSource, ReminderOffset,
    TaskPriority, WorkType,
};
use rust_decimal::Decimal;
use tempfile::TempDir;

#[test]
fn profile_isolation_and_subject_ambiguity() {
    let dir = TempDir::new().unwrap();
    let app = AppServices::open_file(dir.path().join("test.db")).unwrap();

    let p1 = app.get_active_profile().unwrap();
    let p2 = app
        .create_profile(
            CreateProfileInput {
                name: "Онлайн-курсы".into(),
                icon: None,
                color: None,
                institution: Some("Stepik".into()),
                group_name: None,
                academic_year: None,
                description: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    let s1 = app
        .create_subject(
            CreateSubjectInput {
                profile_id: p1.id,
                name: "Программирование на Rust".into(),
                short_name: Some("Rust".into()),
                teacher: None,
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: Some(Decimal::new(45, 1)),
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let _s2 = app
        .create_subject(
            CreateSubjectInput {
                profile_id: p1.id,
                name: "Программирование на C++".into(),
                short_name: Some("C++".into()),
                teacher: None,
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    // Ambiguous query returns both candidates instead of picking one randomly
    let lookup = app.find_subject(p1.id, "Программирование").unwrap();
    assert!(lookup.ambiguous);
    assert!(lookup.exact.is_none());
    assert_eq!(lookup.candidates.len(), 2);

    // Unambiguous short name resolves directly
    let exact = app.find_subject(p1.id, "Rust").unwrap();
    assert!(!exact.ambiguous);
    assert_eq!(exact.exact.unwrap().id, s1.id);

    // Cross-profile write MUST be rejected
    let cross = app.add_grade(
        AddGradeInput {
            profile_id: p2.id,
            subject_id: s1.id,
            date: NaiveDate::from_ymd_opt(2026, 11, 10).unwrap(),
            time: None,
            mark_type_id: None,
            mark_code: None,
            numeric_grade: Some(5),
            weight: None,
            work_type: None,
            topic: None,
            comment: None,
            teacher: None,
        },
        RecordSource::Mcp,
        None,
    );
    assert!(cross.is_err());
}

#[test]
fn grades_statistics_forecast_undo_and_honest_attendance() {
    let dir = TempDir::new().unwrap();
    let app = AppServices::open_file(dir.path().join("stats.db")).unwrap();
    let p = app.get_active_profile().unwrap();
    let subj = app
        .create_subject(
            CreateSubjectInput {
                profile_id: p.id,
                name: "Философия".into(),
                short_name: Some("Фил".into()),
                teacher: Some("Сократов А.А.".into()),
                color: Some("#2E7D32".into()),
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: Some(Decimal::new(45, 1)),
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    let d1 = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    let d2 = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();

    // Multiple grades on the same day + attendance on the same day
    app.add_grade(
        AddGradeInput {
            profile_id: p.id,
            subject_id: subj.id,
            date: d1,
            time: None,
            mark_type_id: None,
            mark_code: None,
            numeric_grade: Some(4),
            weight: Some(Decimal::ONE),
            work_type: Some(WorkType::ClassAnswer),
            topic: Some("Античная философия".into()),
            comment: None,
            teacher: None,
        },
        RecordSource::Gui,
        None,
    )
    .unwrap();
    let g2 = app
        .add_grade(
            AddGradeInput {
                profile_id: p.id,
                subject_id: subj.id,
                date: d1,
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(4),
                weight: Some(Decimal::from(2)),
                work_type: Some(WorkType::ControlWork),
                topic: Some("Контрольная".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    app.add_attendance(
        AddAttendanceInput {
            profile_id: p.id,
            subject_id: subj.id,
            date: d2,
            time: None,
            status: AttendanceStatus::Excused,
            reason: Some("Олимпиада".into()),
            comment: None,
        },
        RecordSource::Mcp,
        Some(&AuditMeta::mcp(Some("Gemini".into()), "add_attendance")),
    )
    .unwrap();

    // Journal cell contains 2 grades on d1 and 1 attendance on d2
    let matrix = app.get_monthly_journal(p.id, 2026, 11, d2).unwrap();
    assert_eq!(matrix.rows.len(), 1);
    assert_eq!(matrix.rows[0].cells_by_date[&d1].grades.len(), 2);
    assert_eq!(matrix.rows[0].cells_by_date[&d2].attendance.len(), 1);
    assert_eq!(matrix.rows[0].average_formatted, "4.00");

    // Statistics: honest attendance reports no percentage when no `present` sessions logged
    let report = app
        .get_statistics(&StatisticsFilter {
            profile_id: p.id,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.numeric_grade_count, 2);
    assert_eq!(report.excused_absences, 1);
    assert!(!report.attendance_summary.has_sufficient_session_data);
    assert!(report.attendance_summary.attendance_percent.is_none());

    // Forecast: current [4, 4] -> need 2 fives to reach 4.5 (arithmetic), never persists to DB
    let reach = app
        .forecast_needed_for_target(
            p.id,
            Some(subj.id),
            None,
            None,
            Decimal::new(45, 1),
            Decimal::from(5),
            Decimal::ONE,
            Some(5),
            Some(AverageMode::Arithmetic),
        )
        .unwrap();
    assert!(reach.reachable);
    assert_eq!(reach.grades_needed, Some(2));
    assert!(reach.is_forecast);
    assert_eq!(
        app.list_grades(p.id, Some(subj.id), None, None, None, None, None, None)
            .unwrap()
            .len(),
        2
    );

    let proj = app
        .forecast_grades(
            p.id,
            Some(subj.id),
            None,
            None,
            &[GradeSample::new(Decimal::from(5), Decimal::from(3))],
            Some(AverageMode::Weighted),
        )
        .unwrap();
    // (4*1 + 4*2 + 5*3) / (1+2+3) = 27/6 = 4.5
    assert_eq!(proj.projected_average, Some(Decimal::new(45, 1)));

    // GUI Undo removes g2, Redo restores g2
    assert!(app.undo_last().unwrap());
    assert!(app.get_grade(g2.id).is_err());
    assert!(app.redo_last().unwrap());
    assert!(app.get_grade(g2.id).is_ok());

    // Activity Log Undo reverses MCP attendance entry
    let mcp_logs = app
        .list_activity_log(Some(p.id), Some(RecordSource::Mcp), 10)
        .unwrap();
    assert_eq!(mcp_logs.len(), 1);
    assert!(app.undo_activity_entry(mcp_logs[0].id).unwrap());
    assert!(
        app.list_attendance(p.id, None, None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn kanban_tags_reminders_export_import_backup_and_secrets() {
    let dir = TempDir::new().unwrap();
    let app = AppServices::open_file(dir.path().join("full.db")).unwrap();
    let p = app.get_active_profile().unwrap();
    let subj = app
        .create_subject(
            CreateSubjectInput {
                profile_id: p.id,
                name: "Базы данных".into(),
                short_name: Some("БД".into()),
                teacher: None,
                color: None,
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    // Case-insensitive tag deduplication
    let t1 = app
        .get_or_create_tag(p.id, "Лабораторная", None, RecordSource::Gui)
        .unwrap();
    let t2 = app
        .get_or_create_tag(p.id, "лабораторная", None, RecordSource::Mcp)
        .unwrap();
    assert_eq!(t1.id, t2.id);

    // Create task with due reminder already due -> triggers notification
    let task = app
        .create_task(
            CreateTaskInput {
                profile_id: p.id,
                board_id: None,
                column_id: None,
                subject_id: Some(subj.id),
                title: "Лабораторная №4".into(),
                description: Some("SQL индексы".into()),
                priority: Some(TaskPriority::High),
                color: None,
                start_date: None,
                due_at: Some(Utc::now() - Duration::minutes(5)),
                notes: None,
                tag_names: vec!["Лабораторная".into()],
                badge_names: vec!["Срочно".into()],
                checklist_titles: vec!["Схема".into(), "Запросы".into()],
                reminder_offsets: vec![ReminderOffset::AtDue],
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();

    // Turn off OS toast during automated tests
    let mut settings = app.get_settings().unwrap();
    settings.os_notifications_enabled = false;
    app.update_settings(&settings, RecordSource::Gui).unwrap();

    let fired = app.process_due_reminders().unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(app.list_notifications(false, 10).unwrap().len(), 1);

    // Link grade to task and complete task
    let grade = app
        .add_grade(
            AddGradeInput {
                profile_id: p.id,
                subject_id: subj.id,
                date: NaiveDate::from_ymd_opt(2026, 11, 25).unwrap(),
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(5),
                weight: None,
                work_type: Some(WorkType::LabWork),
                topic: Some("Индексы".into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    app.link_task_grade(task.id, grade.id, RecordSource::Gui)
        .unwrap();
    let done = app.complete_task(task.id, RecordSource::Gui, None).unwrap();
    assert!(done.is_completed());
    assert_eq!(done.linked_grade_ids, vec![grade.id]);

    // Add an attendance record before export so JSON export->import preserves attendance too
    app.add_attendance(
        AddAttendanceInput {
            profile_id: p.id,
            subject_id: subj.id,
            date: NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
            time: None,
            status: AttendanceStatus::Absent,
            reason: Some("Пропуск пары".into()),
            comment: None,
        },
        RecordSource::Gui,
        None,
    )
    .unwrap();

    // Export JSON, CSV, XLSX, Markdown, and ZIP backup
    let json_path = dir.path().join("export.json");
    let csv_path = dir.path().join("export.csv");
    let xlsx_path = dir.path().join("export.xlsx");
    let md_path = dir.path().join("export.md");
    app.export_profile_json(p.id, &json_path).unwrap();
    app.export_grades_csv(p.id, &csv_path).unwrap();
    app.export_profile_xlsx(p.id, &xlsx_path).unwrap();
    app.export_profile_markdown(p.id, &md_path).unwrap();
    assert!(xlsx_path.metadata().unwrap().len() > 1000);
    assert!(
        std::fs::read_to_string(&md_path)
            .unwrap()
            .contains("Базы данных")
    );

    // Preview & Import JSON into a second profile (verifies grades + attendance + tasks!)
    let preview = app.preview_json_import(&json_path).unwrap();
    assert_eq!(preview.grades_count, 1);
    assert_eq!(preview.attendance_count, 1);
    assert_eq!(preview.tasks_count, 1);

    let p2 = app
        .create_profile(
            CreateProfileInput {
                name: "Импорт проверка".into(),
                icon: None,
                color: None,
                institution: None,
                group_name: None,
                academic_year: None,
                description: None,
            },
            RecordSource::Gui,
            None,
        )
        .unwrap();
    let imported = app.import_json_into_profile(p2.id, &json_path).unwrap();
    assert_eq!(imported.grades_count, 1);
    assert_eq!(imported.attendance_count, 1);
    assert_eq!(imported.tasks_count, 1);
    assert_eq!(
        app.list_attendance(p2.id, None, None, None).unwrap().len(),
        1
    );

    // Telegram Desktop export preview
    let tg_path = dir.path().join("tg_result.json");
    std::fs::write(
        &tg_path,
        r#"{"name":"Saved Messages","messages":[{"id":1,"date":"2026-11-20T10:00:00","text":"ДЗ по философии стр 42"},{"id":2,"date":"2026-11-21T12:00:00","text":[{"type":"plain","text":"Лаба 5 по БД"}]}]}"#,
    )
    .unwrap();
    let tg_prev = app.preview_telegram_export(&tg_path).unwrap();
    assert_eq!(tg_prev.non_empty_notes.len(), 2);

    // Full ZIP backup + restore
    let backup = app.create_backup_archive(None).unwrap();
    assert!(backup.path.exists());
    app.delete_grade(grade.id, RecordSource::Gui, None).unwrap();
    assert!(
        app.list_grades(p.id, None, None, None, None, None, None, None)
            .unwrap()
            .is_empty()
    );
    app.restore_from_backup_archive(&backup.path).unwrap();
    assert_eq!(
        app.list_grades(p.id, None, None, None, None, None, None, None)
            .unwrap()
            .len(),
        1
    );

    // Secret token management & redaction
    let tok = app.tokens.load_or_create().unwrap();
    assert!(app.tokens.verify_bearer(&tok));
    assert!(!app.tokens.verify_bearer("oxd_wrong"));
    let redacted = redact_secrets(&format!("Bearer {tok} leaked"));
    assert!(!redacted.contains(&tok));
    assert!(redacted.contains("[REDACTED_TOKEN]"));

    // Russian pluralization
    assert_eq!(i18n::plural_grades(Language::Ru, 1), "1 оценка");
    assert_eq!(i18n::plural_grades(Language::Ru, 3), "3 оценки");
    assert_eq!(i18n::plural_grades(Language::Ru, 11), "11 оценок");
    assert_eq!(i18n::plural_grades(Language::Ru, 25), "25 оценок");
}
