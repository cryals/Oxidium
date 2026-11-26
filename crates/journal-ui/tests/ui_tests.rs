use journal_app::AppServices;
use journal_mcp::EmbeddedHttpManager;
use journal_ui::{MainWindow, OxidiumTheme, UiController};
use slint::{ComponentHandle, Model};
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn headless_ui_end_to_end_flows() {
    i_slint_backend_testing::init_no_event_loop();

    let dir = tempdir().expect("tempdir");
    let app = AppServices::open_file(dir.path().join("ui-e2e.db")).expect("open app");
    let mcp = Arc::new(EmbeddedHttpManager::new(Arc::clone(&app)));
    let controller = UiController::new(Arc::clone(&app), Arc::clone(&mcp), None);

    let win = MainWindow::new().expect("create MainWindow");
    controller.bind_to_window(&win);

    // 1. Clean production startup: NO fake/mock subjects, average is "—"
    assert_eq!(win.get_journal_rows().row_count(), 0);
    assert_eq!(win.get_journal_overall_avg().as_str(), "—");
    assert_eq!(win.get_active_section(), 0);
    assert_eq!(win.get_tr().nav_journal.as_str(), "Журнал");

    // 2. Create a subject through UI callback (name, short, teacher, target_str, color)
    win.invoke_subject_create(
        "Философия".into(),
        "Фил".into(),
        "Сократов А.А.".into(),
        "4.5".into(),
        "#2E7D32".into(),
    );
    assert_eq!(win.get_journal_rows().row_count(), 1);
    let subj_row = win.get_journal_rows().row_data(0).expect("row 0");
    assert_eq!(subj_row.subject_name.as_str(), "Философия");
    assert_eq!(subj_row.average_text.as_str(), "—");

    // 3. Add numeric grades (5 and 4) and an absence (must NOT distort average!)
    let today_iso = chrono::Utc::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    win.invoke_journal_quick_mark(
        subj_row.subject_id.clone(),
        today_iso.clone().into(),
        "grade_5".into(),
    );
    win.invoke_journal_quick_mark(
        subj_row.subject_id.clone(),
        today_iso.clone().into(),
        "grade_4".into(),
    );
    win.invoke_journal_quick_mark(
        subj_row.subject_id.clone(),
        today_iso.into(),
        "absent".into(),
    );

    let updated_row = win.get_journal_rows().row_data(0).expect("row 0 updated");
    assert_eq!(updated_row.average_text.as_str(), "4.50");
    assert_eq!(win.get_journal_overall_avg().as_str(), "4.50");

    // 4. Switch to Statistics (section 1) and run non-destructive forecast
    win.set_active_section(1);
    win.invoke_section_changed(1);
    assert_eq!(win.get_stats_overall_avg().as_str(), "4.50");
    assert_eq!(win.get_stats_total_grades().as_str(), "2");
    assert_eq!(win.get_stats_absences().as_str(), "1");

    win.invoke_stats_run_forecast(0, "5, 5".into(), "4.8".into(), "5".into(), "1".into());
    assert!(!win.get_forecast_result_text().is_empty());
    assert!(!win.get_forecast_target_result_text().is_empty());
    // Forecast MUST NOT write to database
    assert_eq!(win.get_stats_total_grades().as_str(), "2");

    // 5. Switch to Kanban (section 2), create task, move it, toggle complete
    win.set_active_section(2);
    win.invoke_section_changed(2);
    let cols_before = win.get_kanban_columns();
    assert!(cols_before.row_count() >= 4);
    let col0_id = cols_before.row_data(0).expect("col 0").id;

    win.invoke_kanban_create_task(
        col0_id,
        "Написать эссе по Канту".into(),
        "Глава 1-3".into(),
        subj_row.subject_id.clone(),
        2,
        3,
        "Эссе, Важное".into(),
    );
    let col0_after = win.get_kanban_columns().row_data(0).expect("col 0 after");
    assert_eq!(col0_after.tasks.row_count(), 1);
    let task = col0_after.tasks.row_data(0).expect("task 0");
    assert_eq!(task.title.as_str(), "Написать эссе по Канту");

    // Move task one column right
    win.invoke_kanban_move_task_step(task.id.clone(), 1);
    assert_eq!(
        win.get_kanban_columns()
            .row_data(0)
            .unwrap()
            .tasks
            .row_count(),
        0
    );
    assert_eq!(
        win.get_kanban_columns()
            .row_data(1)
            .unwrap()
            .tasks
            .row_count(),
        1
    );

    // Toggle task complete
    win.invoke_kanban_toggle_task_complete(task.id);
    win.invoke_section_changed(1);
    assert_eq!(win.get_stats_completed_tasks().as_str(), "1");

    // 6. Global Search finds both subject and task
    win.invoke_search_query_changed("Канту".into());
    assert!(win.get_search_hits().row_count() >= 1);

    // 7. Appearance settings persistence & EN/RU localization switch
    win.invoke_save_appearance(1, 1, "#4A4459".into(), true, true, 110);
    assert!(win.global::<OxidiumTheme>().get_dark_mode());
    assert_eq!(win.get_tr().nav_journal.as_str(), "Journal");
    let persisted = app.get_settings().expect("get_settings");
    assert_eq!(persisted.accent_color, "#4A4459");
    assert!(persisted.compact_mode);
    assert!(persisted.reduced_motion);

    // Switch back to Russian + Light
    win.invoke_save_appearance(0, 0, "#4A4459".into(), false, false, 100);
    assert!(!win.global::<OxidiumTheme>().get_dark_mode());
    assert_eq!(win.get_tr().nav_journal.as_str(), "Журнал");

    // 8. Profile isolation: create second profile -> switches cleanly with 0 subjects
    win.invoke_profile_create(
        "Университет".into(),
        "МГТУ".into(),
        "ИУ7-31Б".into(),
        "2026/2027".into(),
        "#0F766E".into(),
    );
    assert_eq!(win.get_active_profile_name().as_str(), "Университет");
    assert_eq!(win.get_journal_rows().row_count(), 0);
    assert_eq!(win.get_journal_overall_avg().as_str(), "—");

    // 9. 100-subject UI stress check in the new profile
    let p2 = app.get_active_profile().expect("p2");
    for i in 0..100 {
        app.create_subject(
            journal_app::CreateSubjectInput {
                profile_id: p2.id,
                name: format!("Предмет #{:03}", i + 1),
                short_name: Some(format!("П{:02}", i + 1)),
                teacher: None,
                color: Some("#4A4459".into()),
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: None,
            },
            journal_domain::RecordSource::Gui,
            None,
        )
        .expect("create stress subject");
    }
    let t_ui = std::time::Instant::now();
    controller.refresh_journal(&win);
    let ui_ms = t_ui.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(win.get_journal_rows().row_count(), 100);
    println!("[UI-STRESS] refresh_journal(100 subjects) = {ui_ms:.2}ms");
}
