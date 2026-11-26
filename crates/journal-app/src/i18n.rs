use chrono::{NaiveDate, Weekday};
use journal_domain::{Language, TaskPriority, WorkType};

/// Russian 3-form pluralization: 1 оценка / 2 оценки / 5 оценок
pub fn plural_ru<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let mod100 = n % 100;
    let mod10 = n % 10;
    if (11..=14).contains(&mod100) {
        many
    } else if mod10 == 1 {
        one
    } else if (2..=4).contains(&mod10) {
        few
    } else {
        many
    }
}

pub fn plural_grades(lang: Language, n: usize) -> String {
    match lang {
        Language::Ru => format!("{n} {}", plural_ru(n, "оценка", "оценки", "оценок")),
        Language::En => format!("{n} {}", if n == 1 { "grade" } else { "grades" }),
    }
}

pub fn plural_tasks(lang: Language, n: usize) -> String {
    match lang {
        Language::Ru => format!("{n} {}", plural_ru(n, "задача", "задачи", "задач")),
        Language::En => format!("{n} {}", if n == 1 { "task" } else { "tasks" }),
    }
}

pub fn month_name(lang: Language, month: u32) -> &'static str {
    match (lang, month) {
        (Language::Ru, 1) => "Январь",
        (Language::Ru, 2) => "Февраль",
        (Language::Ru, 3) => "Март",
        (Language::Ru, 4) => "Апрель",
        (Language::Ru, 5) => "Май",
        (Language::Ru, 6) => "Июнь",
        (Language::Ru, 7) => "Июль",
        (Language::Ru, 8) => "Август",
        (Language::Ru, 9) => "Сентябрь",
        (Language::Ru, 10) => "Октябрь",
        (Language::Ru, 11) => "Ноябрь",
        (Language::Ru, 12) => "Декабрь",
        (Language::En, 1) => "January",
        (Language::En, 2) => "February",
        (Language::En, 3) => "March",
        (Language::En, 4) => "April",
        (Language::En, 5) => "May",
        (Language::En, 6) => "June",
        (Language::En, 7) => "July",
        (Language::En, 8) => "August",
        (Language::En, 9) => "September",
        (Language::En, 10) => "October",
        (Language::En, 11) => "November",
        (Language::En, 12) => "December",
        _ => "",
    }
}

pub fn weekday_short(lang: Language, wd: Weekday) -> &'static str {
    match (lang, wd) {
        (Language::Ru, Weekday::Mon) => "Пн",
        (Language::Ru, Weekday::Tue) => "Вт",
        (Language::Ru, Weekday::Wed) => "Ср",
        (Language::Ru, Weekday::Thu) => "Чт",
        (Language::Ru, Weekday::Fri) => "Пт",
        (Language::Ru, Weekday::Sat) => "Сб",
        (Language::Ru, Weekday::Sun) => "Вс",
        (Language::En, Weekday::Mon) => "Mo",
        (Language::En, Weekday::Tue) => "Tu",
        (Language::En, Weekday::Wed) => "We",
        (Language::En, Weekday::Thu) => "Th",
        (Language::En, Weekday::Fri) => "Fr",
        (Language::En, Weekday::Sat) => "Sa",
        (Language::En, Weekday::Sun) => "Su",
    }
}

pub fn work_type_label(lang: Language, wt: WorkType) -> &'static str {
    match (lang, wt) {
        (Language::Ru, WorkType::ClassAnswer) => "Ответ на занятии",
        (Language::Ru, WorkType::Homework) => "Домашняя работа",
        (Language::Ru, WorkType::ControlWork) => "Контрольная работа",
        (Language::Ru, WorkType::IndependentWork) => "Самостоятельная работа",
        (Language::Ru, WorkType::LabWork) => "Лабораторная работа",
        (Language::Ru, WorkType::PracticalWork) => "Практическая работа",
        (Language::Ru, WorkType::Test) => "Тест",
        (Language::Ru, WorkType::Project) => "Проект",
        (Language::Ru, WorkType::Exam) => "Экзамен",
        (Language::Ru, WorkType::Credit) => "Зачёт",
        (Language::Ru, WorkType::Other) => "Другое",
        (Language::En, WorkType::ClassAnswer) => "Class Answer",
        (Language::En, WorkType::Homework) => "Homework",
        (Language::En, WorkType::ControlWork) => "Control Work",
        (Language::En, WorkType::IndependentWork) => "Independent Work",
        (Language::En, WorkType::LabWork) => "Lab Work",
        (Language::En, WorkType::PracticalWork) => "Practical Work",
        (Language::En, WorkType::Test) => "Test",
        (Language::En, WorkType::Project) => "Project",
        (Language::En, WorkType::Exam) => "Exam",
        (Language::En, WorkType::Credit) => "Credit",
        (Language::En, WorkType::Other) => "Other",
    }
}

pub fn priority_label(lang: Language, p: TaskPriority) -> &'static str {
    match (lang, p) {
        (Language::Ru, TaskPriority::None) => "Без приоритета",
        (Language::Ru, TaskPriority::Low) => "Низкий",
        (Language::Ru, TaskPriority::Medium) => "Средний",
        (Language::Ru, TaskPriority::High) => "Высокий",
        (Language::Ru, TaskPriority::Critical) => "Критический",
        (Language::En, TaskPriority::None) => "No priority",
        (Language::En, TaskPriority::Low) => "Low",
        (Language::En, TaskPriority::Medium) => "Medium",
        (Language::En, TaskPriority::High) => "High",
        (Language::En, TaskPriority::Critical) => "Critical",
    }
}

pub fn default_column_names(lang: Language) -> [(&'static str, &'static str, bool); 4] {
    match lang {
        Language::Ru => [
            ("Запланировано", "#64748B", false),
            ("В процессе", "#2563EB", false),
            ("На проверке", "#D97706", false),
            ("Выполнено", "#16A34A", true),
        ],
        Language::En => [
            ("Planned", "#64748B", false),
            ("In Progress", "#2563EB", false),
            ("In Review", "#D97706", false),
            ("Done", "#16A34A", true),
        ],
    }
}

pub fn tr(lang: Language, key: &str) -> &'static str {
    match (lang, key) {
        (Language::Ru, "nav.journal") => "Журнал",
        (Language::Ru, "nav.statistics") => "Статистика",
        (Language::Ru, "nav.kanban") => "Канбан",
        (Language::Ru, "nav.profiles") => "Профили",
        (Language::Ru, "nav.settings") => "Настройки",
        (Language::Ru, "nav.mcp") => "MCP-сервер",
        (Language::Ru, "nav.notifications") => "Уведомления",
        (Language::Ru, "journal.subject") => "Предмет",
        (Language::Ru, "journal.average") => "Средний",
        (Language::Ru, "journal.today") => "Сегодня",
        (Language::Ru, "journal.week") => "Неделя",
        (Language::Ru, "journal.month") => "Месяц",
        (Language::Ru, "journal.semester") => "Семестр",
        (Language::Ru, "journal.year") => "Учебный год",
        (Language::Ru, "journal.custom") => "Период",
        (Language::Ru, "journal.add_subject") => "Добавить предмет",
        (Language::Ru, "journal.empty") => {
            "Нет предметов. Добавьте первый предмет, чтобы начать вести журнал."
        }
        (Language::Ru, "stats.overall_avg") => "Общий средний балл",
        (Language::Ru, "stats.total_grades") => "Всего оценок",
        (Language::Ru, "stats.absences") => "Пропуски (Н/Б)",
        (Language::Ru, "stats.excused") => "Уважительные",
        (Language::Ru, "stats.passes") => "Зачёты",
        (Language::Ru, "stats.completed_tasks") => "Выполнено задач",
        (Language::Ru, "stats.overdue_tasks") => "Просрочено задач",
        (Language::Ru, "stats.forecast_badge") => "Прогноз — не записывается в журнал",
        (Language::Ru, "stats.attendance_no_total") => {
            "Показаны зарегистрированные пропуски (общее число занятий не задано)"
        }
        (Language::Ru, "kanban.new_task") => "Новая задача",
        (Language::Ru, "kanban.new_column") => "Новая колонка",
        (Language::Ru, "kanban.new_board") => "Новая доска",
        (Language::Ru, "search.placeholder") => "Поиск предметов, оценок, задач, тегов... (Ctrl+F)",
        (Language::Ru, "mcp.running") => "Запущен",
        (Language::Ru, "mcp.stopped") => "Остановлен",
        (Language::Ru, "mcp.error") => "Ошибка",
        (Language::Ru, "mcp.privacy_notice") => {
            "Подключённый AI-ассистент получает доступ к учебным данным строго в пределах выбранных разрешений."
        }
        (Language::Ru, "reminders.background_notice") => {
            "Напоминания доставляются, пока запущен Oxidium. При закрытом приложении уведомления не отправляются."
        }

        (Language::En, "nav.journal") => "Journal",
        (Language::En, "nav.statistics") => "Statistics",
        (Language::En, "nav.kanban") => "Kanban",
        (Language::En, "nav.profiles") => "Profiles",
        (Language::En, "nav.settings") => "Settings",
        (Language::En, "nav.mcp") => "MCP Server",
        (Language::En, "nav.notifications") => "Notifications",
        (Language::En, "journal.subject") => "Subject",
        (Language::En, "journal.average") => "Average",
        (Language::En, "journal.today") => "Today",
        (Language::En, "journal.week") => "Week",
        (Language::En, "journal.month") => "Month",
        (Language::En, "journal.semester") => "Semester",
        (Language::En, "journal.year") => "Academic Year",
        (Language::En, "journal.custom") => "Custom",
        (Language::En, "journal.add_subject") => "Add Subject",
        (Language::En, "journal.empty") => {
            "No subjects yet. Add your first subject to start tracking grades."
        }
        (Language::En, "stats.overall_avg") => "Overall Average",
        (Language::En, "stats.total_grades") => "Total Grades",
        (Language::En, "stats.absences") => "Absences",
        (Language::En, "stats.excused") => "Excused",
        (Language::En, "stats.passes") => "Passes",
        (Language::En, "stats.completed_tasks") => "Completed Tasks",
        (Language::En, "stats.overdue_tasks") => "Overdue Tasks",
        (Language::En, "stats.forecast_badge") => "Forecast — not saved to journal",
        (Language::En, "stats.attendance_no_total") => {
            "Showing recorded absences only (total scheduled classes not configured)"
        }
        (Language::En, "kanban.new_task") => "New Task",
        (Language::En, "kanban.new_column") => "New Column",
        (Language::En, "kanban.new_board") => "New Board",
        (Language::En, "search.placeholder") => "Search subjects, grades, tasks, tags... (Ctrl+F)",
        (Language::En, "mcp.running") => "Running",
        (Language::En, "mcp.stopped") => "Stopped",
        (Language::En, "mcp.error") => "Error",
        (Language::En, "mcp.privacy_notice") => {
            "Connected AI assistants can access your academic data strictly within the permissions you grant."
        }
        (Language::En, "reminders.background_notice") => {
            "Reminders are delivered while Oxidium is running. Closed-app background delivery is not enabled."
        }
        _ => "",
    }
}

pub fn format_month_year(lang: Language, year: i32, month: u32) -> String {
    format!("{} {}", month_name(lang, month), year)
}

pub fn format_short_date(lang: Language, date: NaiveDate) -> String {
    match lang {
        Language::Ru => date.format("%d.%m.%Y").to_string(),
        Language::En => date.format("%Y-%m-%d").to_string(),
    }
}
