use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use journal_app::{
    AddAttendanceInput, AddGradeInput, AppServices, AuditMeta, CreateSubjectInput, CreateTaskInput,
};
use journal_domain::{AttendanceStatus, RecordSource, TaskPriority, ThemeMode, WorkType};
use journal_mcp::EmbeddedHttpManager;
use rust_decimal::Decimal;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use crate::MainWindow;
use crate::controller::UiController;

struct HeadlessSoftwarePlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl slint::platform::Platform for HeadlessSoftwarePlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}

fn save_rgb_png(path: &Path, width: u32, height: u32, pixels: &[Rgb8Pixel]) -> Result<()> {
    let mut raw = Vec::with_capacity(pixels.len() * 3);
    for px in pixels {
        raw.extend_from_slice(&[px.r, px.g, px.b]);
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&raw)?;
    Ok(())
}

/// Renders high-resolution real screenshots of all Oxidium screens using an isolated temporary
/// database (ONLY invoked via `--render-screenshots <dir>`, never used in normal app startup).
pub fn render_screenshots_headless(out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let width = 1440u32;
    let height = 900u32;

    let sw_win = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    sw_win.set_size(PhysicalSize::new(width, height));
    slint::platform::set_platform(Box::new(HeadlessSoftwarePlatform {
        window: sw_win.clone(),
    }))
    .map_err(|e| anyhow::anyhow!("set_platform failed: {e}"))?;

    // Isolated temporary DB strictly for visual QA & README screenshots
    let tmp_dir = std::env::temp_dir().join(format!("oxidium-shot-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir)?;
    let app = AppServices::open_file(tmp_dir.join("qa.db"))?;
    seed_visual_qa_dataset(&app)?;

    let mcp = Arc::new(EmbeddedHttpManager::new(Arc::clone(&app)));
    let controller = UiController::new(Arc::clone(&app), Arc::clone(&mcp), None);
    let win = MainWindow::new()?;
    controller.bind_to_window(&win);
    win.show()?;

    let mut buffer = vec![Rgb8Pixel::default(); (width * height) as usize];
    let mut capture = |filename: &str| -> Result<()> {
        slint::platform::update_timers_and_animations();
        sw_win.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, width as usize);
        });
        // Force a second pass in case initial layout settled on first frame
        sw_win.request_redraw();
        sw_win.draw_if_needed(|renderer| {
            renderer.render(&mut buffer, width as usize);
        });
        save_rgb_png(&out_dir.join(filename), width, height, &buffer)?;
        Ok(())
    };

    // 1. Journal Light Mode
    win.set_active_section(0);
    controller.refresh_all(&win);
    win.invoke_apply_color_scheme(0);
    capture("journal-light.png")?;

    // 2. Journal Dark Mode
    if let Ok(mut s) = app.get_settings() {
        s.theme_mode = ThemeMode::Dark;
        let _ = app.update_settings(&s, RecordSource::Gui);
    }
    controller.apply_settings_and_theme(&win);
    win.invoke_apply_color_scheme(1);
    controller.refresh_all(&win);
    capture("journal-dark.png")?;

    // 3. Statistics & Real Charts (Light)
    if let Ok(mut s) = app.get_settings() {
        s.theme_mode = ThemeMode::Light;
        let _ = app.update_settings(&s, RecordSource::Gui);
    }
    controller.apply_settings_and_theme(&win);
    win.set_active_section(1);
    controller.refresh_statistics(&win);
    win.invoke_apply_color_scheme(0);
    capture("statistics.png")?;

    // 4. Kanban Board (Light)
    win.set_active_section(2);
    controller.refresh_kanban(&win);
    win.invoke_apply_color_scheme(0);
    capture("kanban.png")?;

    // 5. Settings & MCP Server Panel (Dark)
    if let Ok(mut s) = app.get_settings() {
        s.theme_mode = ThemeMode::Dark;
        let _ = app.update_settings(&s, RecordSource::Gui);
    }
    controller.apply_settings_and_theme(&win);
    win.set_active_section(0);
    win.set_settings_active_tab(3);
    win.set_settings_modal_open(true);
    controller.refresh_mcp_panel(&win);
    win.invoke_apply_color_scheme(1);
    capture("settings-mcp.png")?;

    // 6. Statistics Dark Mode
    win.set_settings_modal_open(false);
    win.set_active_section(1);
    controller.refresh_statistics(&win);
    win.invoke_apply_color_scheme(1);
    capture("statistics-dark.png")?;

    // 7. Kanban Dark Mode
    win.set_active_section(2);
    controller.refresh_kanban(&win);
    win.invoke_apply_color_scheme(1);
    capture("kanban-dark.png")?;

    // 8. 4-Step Expressive Setup Wizard (Dark)
    win.set_active_section(0);
    win.set_onboarding_step(1);
    win.set_onboarding_open(true);
    win.invoke_apply_color_scheme(1);
    capture("onboarding.png")?;

    // 9. Dynamic GitHub Releases Update Modal + Top-Bar Update Pill (Dark)
    win.set_onboarding_open(false);
    controller.apply_update_info(
        &win,
        journal_app::UpdateInfo {
            current_version: journal_app::CURRENT_APP_VERSION.to_string(),
            latest_version: "0.2.0".to_string(),
            tag_name: "v0.2.0".to_string(),
            release_name: "Oxidium v0.2.0".to_string(),
            release_notes: "• Ускорена загрузка матрицы оценок на больших базах.\n• Добавлен расширенный экспорт статистики в Markdown и XLSX.\n• Улучшена синхронизация MCP-сервера с Claude Desktop и Gemini CLI.\n• Автоматическая проверка целостности SHA-256 перед установкой.".to_string(),
            html_url: journal_app::GITHUB_RELEASES_PAGE.to_string(),
            published_at: "2026-12-01".to_string(),
            selected_asset: Some(journal_app::ReleaseAssetInfo {
                name: "Oxidium-0.2.0-windows-x86_64-setup.exe".to_string(),
                download_url: "https://github.com/cryals/Oxidium/releases/download/v0.2.0/Oxidium-0.2.0-windows-x86_64-setup.exe".to_string(),
                size_bytes: 16_450_000,
            }),
            sha256_expected: Some("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".to_string()),
            is_update_available: true,
        },
    );
    win.set_update_modal_open(true);
    win.invoke_apply_color_scheme(1);
    capture("update-modal.png")?;

    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

fn seed_visual_qa_dataset(app: &AppServices) -> Result<()> {
    let p = app.get_active_profile()?;
    let _ = app.update_profile(
        p.id,
        journal_app::UpdateProfileInput {
            name: Some("Алексей Соколов".into()),
            icon: None,
            color: Some("#4A4459".into()),
            institution: Some("ИТМО".into()),
            group_name: Some("ИТ-24".into()),
            academic_year: Some("2026/2027".into()),
            description: None,
        },
        RecordSource::Gui,
        None,
    );
    let mut settings = app.get_settings()?;
    settings.theme_mode = ThemeMode::Light;
    settings.accent_color = "#4A4459".to_string();
    app.update_settings(&settings, RecordSource::Gui)?;

    let subjects = [
        (
            "Философия",
            "Фил",
            "Сократов А.А.",
            "#2E7D32",
            Some(Decimal::new(45, 1)),
        ),
        (
            "Базы данных",
            "БД",
            "Королёв В.С.",
            "#2563EB",
            Some(Decimal::new(46, 1)),
        ),
        (
            "Программирование",
            "Прог",
            "Линусов Д.И.",
            "#7C3AED",
            Some(Decimal::new(48, 1)),
        ),
        (
            "Математический анализ",
            "Матан",
            "Эйлеров Н.П.",
            "#D97706",
            Some(Decimal::new(42, 1)),
        ),
        (
            "Операционные системы",
            "ОС",
            "Керниган Р.М.",
            "#0F766E",
            Some(Decimal::new(45, 1)),
        ),
        ("Физкультура", "Физра", "Спортов Е.К.", "#E11D48", None),
    ];

    let mut ids = Vec::new();
    for (name, short, teacher, color, target) in subjects {
        let s = app.create_subject(
            CreateSubjectInput {
                profile_id: p.id,
                name: name.into(),
                short_name: Some(short.into()),
                teacher: Some(teacher.into()),
                color: Some(color.into()),
                icon: None,
                description: None,
                academic_period_id: None,
                target_average: target,
            },
            RecordSource::Gui,
            None,
        )?;
        ids.push(s.id);
    }

    let today = Utc::now().date_naive();
    let y = chrono::Datelike::year(&today);
    let m = chrono::Datelike::month(&today);
    let d = |day: u32| NaiveDate::from_ymd_opt(y, m, day.min(28)).unwrap();

    // Realistic grades across subjects
    let grade_entries = [
        (0, 2, 5, WorkType::ClassAnswer, "Античная философия"),
        (0, 6, 4, WorkType::Homework, "Эссе по Канту"),
        (0, 12, 5, WorkType::Test, "Немецкая классика"),
        (1, 3, 5, WorkType::LabWork, "Нормализация БД"),
        (1, 9, 5, WorkType::LabWork, "Индексы B-Tree"),
        (1, 9, 4, WorkType::ClassAnswer, "Ответ по SQL"),
        (1, 18, 4, WorkType::ControlWork, "Транзакции ACID"),
        (2, 4, 4, WorkType::PracticalWork, "Итераторы Rust"),
        (2, 11, 5, WorkType::LabWork, "Асинхронный Tokio"),
        (2, 19, 5, WorkType::Project, "Сетевой протокол"),
        (3, 5, 3, WorkType::ControlWork, "Ряды Фурье"),
        (3, 14, 4, WorkType::Homework, "Интегралы"),
        (3, 21, 5, WorkType::Test, "Дифференциалы"),
        (4, 7, 5, WorkType::LabWork, "Планировщик процессов"),
        (4, 16, 4, WorkType::Test, "Виртуальная память"),
    ];
    for (si, day, val, wt, topic) in grade_entries {
        app.add_grade(
            AddGradeInput {
                profile_id: p.id,
                subject_id: ids[si],
                date: d(day),
                time: None,
                mark_type_id: None,
                mark_code: None,
                numeric_grade: Some(val),
                weight: None,
                work_type: Some(wt),
                topic: Some(topic.into()),
                comment: None,
                teacher: None,
            },
            RecordSource::Mcp,
            Some(&AuditMeta::mcp(Some("Gemini".into()), "add_grade")),
        )?;
    }

    app.add_grade(
        AddGradeInput {
            profile_id: p.id,
            subject_id: ids[5],
            date: d(15),
            time: None,
            mark_type_id: None,
            mark_code: Some("pass".into()),
            numeric_grade: None,
            weight: None,
            work_type: Some(WorkType::Credit),
            topic: Some("Нормативы".into()),
            comment: None,
            teacher: None,
        },
        RecordSource::Gui,
        None,
    )?;

    app.add_attendance(
        AddAttendanceInput {
            profile_id: p.id,
            subject_id: ids[5],
            date: d(4),
            time: None,
            status: AttendanceStatus::Absent,
            reason: None,
            comment: None,
        },
        RecordSource::Gui,
        None,
    )?;
    app.add_attendance(
        AddAttendanceInput {
            profile_id: p.id,
            subject_id: ids[5],
            date: d(10),
            time: None,
            status: AttendanceStatus::Excused,
            reason: Some("Справка".into()),
            comment: None,
        },
        RecordSource::Mcp,
        Some(&AuditMeta::mcp(Some("Claude".into()), "add_attendance")),
    )?;

    // Kanban tasks across columns
    let boards = app.list_boards(p.id, false)?;
    let cols = app.list_columns(boards[0].id, false)?;
    let tasks = [
        (
            0,
            Some(ids[1]),
            "Лабораторная №4: SQL-индексы",
            TaskPriority::High,
            3,
            vec!["Лабораторная"],
            vec!["Схема таблиц", "EXPLAIN QUERY PLAN", "Отчёт"],
        ),
        (
            0,
            Some(ids[3]),
            "Решить типовой расчёт по рядам",
            TaskPriority::Medium,
            5,
            vec!["Домашнее задание"],
            vec!["Вариант 12"],
        ),
        (
            1,
            Some(ids[2]),
            "Курсовой проект на Rust + Slint",
            TaskPriority::Critical,
            2,
            vec!["Проект"],
            vec!["Доменная модель", "SQLite слой", "MCP-сервер", "Тесты"],
        ),
        (
            2,
            Some(ids[4]),
            "Отчёт по лабораторной ОС №3",
            TaskPriority::Medium,
            -1,
            vec!["На проверке"],
            vec!["Скриншоты", "Выводы"],
        ),
        (
            3,
            Some(ids[0]),
            "Эссе по философии Канта",
            TaskPriority::Low,
            -4,
            vec!["Эссе"],
            vec!["План", "Текст"],
        ),
    ];
    for (ci, sid, title, prio, due_days, tags, checklist) in tasks {
        let t = app.create_task(
            CreateTaskInput {
                profile_id: p.id,
                board_id: Some(boards[0].id),
                column_id: Some(cols[ci].id),
                subject_id: sid,
                title: title.into(),
                description: Some("Учебная задача для визуальной проверки интерфейса".into()),
                priority: Some(prio),
                color: None,
                start_date: None,
                due_at: Some(Utc::now() + Duration::days(due_days)),
                notes: None,
                tag_names: tags.into_iter().map(String::from).collect(),
                badge_names: if ci == 2 {
                    vec!["Срочно".into()]
                } else {
                    vec![]
                },
                checklist_titles: checklist.into_iter().map(String::from).collect(),
                reminder_offsets: vec![],
            },
            RecordSource::Mcp,
            Some(&AuditMeta::mcp(Some("Gemini".into()), "create_task")),
        )?;
        if let Some(first) = t.checklist.first() {
            let _ = app.update_checklist_item(first.id, None, Some(true), None, RecordSource::Gui);
        }
    }

    Ok(())
}
