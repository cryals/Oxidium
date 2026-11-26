use anyhow::Result;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use journal_app::{
    AddAttendanceInput, AddGradeInput, AppServices, CreateAcademicPeriodInput, CreateBoardInput,
    CreateColumnInput, CreateProfileInput, CreateSubjectInput, CreateTaskInput, StatisticsFilter,
    UpdateTaskInput, i18n,
};
use journal_domain::{
    AttendanceStatus, AverageMode, AveragePrecision, CalendarViewMode, GradeSample, Language,
    McpPermissionMode, PeriodType, RecordSource, TaskPriority, ThemeMode, WeekStart, WorkType,
    academic_year_bounds, format_average, month_bounds, next_month, prev_month, week_bounds,
};
use journal_mcp::{EmbeddedHttpManager, McpServerStatus};
use journal_storage::SearchFilter;
use rust_decimal::Decimal;
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::charts::{render_donut_chart, render_line_chart};
use crate::theme::{build_material_schemes, parse_hex_color};
use crate::*;

#[derive(Debug, Clone)]
pub struct UiViewState {
    pub year: i32,
    pub month: u32,
    pub view_mode: CalendarViewMode,
    pub selected_period_id: Option<Uuid>,
    pub active_board_id: Option<Uuid>,
    pub stats_preset: i32,
    pub stats_compare: bool,
    pub kanban_filter_query: String,
    pub kanban_filter_mode: i32,
    pub kanban_subject_filter: Option<Uuid>,
    pub visible_days: Vec<NaiveDate>,
    pub visible_subject_ids: Vec<Uuid>,
}

impl Default for UiViewState {
    fn default() -> Self {
        let today = Utc::now().date_naive();
        Self {
            year: today.year(),
            month: today.month(),
            view_mode: CalendarViewMode::Month,
            selected_period_id: None,
            active_board_id: None,
            stats_preset: 0,
            stats_compare: false,
            kanban_filter_query: String::new(),
            kanban_filter_mode: 0,
            kanban_subject_filter: None,
            visible_days: Vec::new(),
            visible_subject_ids: Vec::new(),
        }
    }
}

pub struct UiController {
    pub app: Arc<AppServices>,
    pub mcp: Arc<EmbeddedHttpManager>,
    pub state: Arc<Mutex<UiViewState>>,
    pub rt_handle: Option<tokio::runtime::Handle>,
}

impl UiController {
    pub fn new(
        app: Arc<AppServices>,
        mcp: Arc<EmbeddedHttpManager>,
        rt_handle: Option<tokio::runtime::Handle>,
    ) -> Arc<Self> {
        Arc::new(Self {
            app,
            mcp,
            state: Arc::new(Mutex::new(UiViewState::default())),
            rt_handle,
        })
    }

    pub fn bind_to_window(self: &Arc<Self>, win: &MainWindow) {
        self.apply_settings_and_theme(win);
        self.refresh_all(win);

        // Auto-start embedded MCP HTTP if configured
        if let Ok(settings) = self.app.get_settings()
            && settings.mcp_http_autostart
            && let Some(ref rt) = self.rt_handle
        {
            let _ = self.mcp.start(rt, settings.mcp_http_port);
            self.refresh_mcp_panel(win);
        }

        // Section switching
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_section_changed(move |sec| {
                if let Some(win) = w.upgrade() {
                    if sec == 1 {
                        ctrl.refresh_statistics(&win);
                    } else if sec == 2 {
                        ctrl.refresh_kanban(&win);
                    } else {
                        ctrl.refresh_journal(&win);
                    }
                }
            });
        }

        // Undo / Redo
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_undo_clicked(move || {
                if let Some(win) = w.upgrade() {
                    match ctrl.app.undo_last() {
                        Ok(true) => {
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Действие отменено");
                        }
                        Ok(false) => {}
                        Err(e) => show_toast(&win, &format!("Ошибка отмены: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_redo_clicked(move || {
                if let Some(win) = w.upgrade() {
                    match ctrl.app.redo_last() {
                        Ok(true) => {
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Действие повторено");
                        }
                        Ok(false) => {}
                        Err(e) => show_toast(&win, &format!("Ошибка повтора: {e}")),
                    }
                }
            });
        }

        // Journal navigation
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_prev_period(move || {
                if let Ok(mut st) = ctrl.state.lock() {
                    let (y, m) = prev_month(st.year, st.month);
                    st.year = y;
                    st.month = m;
                    st.selected_period_id = None;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_journal(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_next_period(move || {
                if let Ok(mut st) = ctrl.state.lock() {
                    let (y, m) = next_month(st.year, st.month);
                    st.year = y;
                    st.month = m;
                    st.selected_period_id = None;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_journal(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_go_today(move || {
                let today = Utc::now().date_naive();
                if let Ok(mut st) = ctrl.state.lock() {
                    st.year = today.year();
                    st.month = today.month();
                    st.selected_period_id = None;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_journal(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_change_view_mode(move |mode| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.view_mode = match mode {
                        0 => CalendarViewMode::Week,
                        2 => CalendarViewMode::Semester,
                        3 => CalendarViewMode::AcademicYear,
                        _ => CalendarViewMode::Month,
                    };
                }
                if let Some(win) = w.upgrade() {
                    win.set_journal_view_mode(mode);
                    ctrl.refresh_journal(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_select_period(move |pid_str| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.selected_period_id = Uuid::parse_str(&pid_str).ok();
                    if st.selected_period_id.is_some() {
                        st.view_mode = CalendarViewMode::Semester;
                    }
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_journal(&win);
                }
            });
        }

        // Cell click -> Quick Grade Picker (empty) OR Detail Modal (filled)
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_cell_clicked(move |sid, date_iso, _r, _c, has_entries| {
                if let Some(win) = w.upgrade() {
                    if has_entries {
                        ctrl.populate_cell_detail(&win, &sid, &date_iso);
                        win.set_journal_cell_detail_open(true);
                    } else {
                        win.set_journal_quick_picker_open(true);
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_open_cell_detail(move |sid, date_iso| {
                if let Some(win) = w.upgrade() {
                    ctrl.populate_cell_detail(&win, &sid, &date_iso);
                    win.set_journal_cell_detail_open(true);
                }
            });
        }

        // Quick mark insertion
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_quick_mark(move |sid_str, date_iso, mark_code| {
                if let Some(win) = w.upgrade() {
                    ctrl.handle_quick_mark(&win, &sid_str, &date_iso, &mark_code);
                }
            });
        }

        // Keyboard 1-5 quick digit grade
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_quick_digit_grade(move |row, col, digit| {
                if let Some(win) = w.upgrade() {
                    let (sid_opt, date_opt) = ctrl
                        .state
                        .lock()
                        .ok()
                        .map(|st| {
                            (
                                st.visible_subject_ids.get(row as usize).copied(),
                                st.visible_days.get(col as usize).copied(),
                            )
                        })
                        .unwrap_or((None, None));
                    if let (Some(sid), Some(date)) = (sid_opt, date_opt) {
                        ctrl.handle_quick_mark(
                            &win,
                            &sid.to_string(),
                            &date.format("%Y-%m-%d").to_string(),
                            &format!("grade_{digit}"),
                        );
                    }
                }
            });
        }

        // Detailed grade / attendance insertion
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_add_detailed_entry(
                move |sid_str, date_iso, mark_label, weight_str, wt_idx, topic, comment| {
                    if let Some(win) = w.upgrade() {
                        let Ok(profile) = ctrl.app.get_active_profile() else {
                            return;
                        };
                        let Ok(sid) = Uuid::parse_str(&sid_str) else {
                            return;
                        };
                        let Ok(date) = NaiveDate::parse_from_str(&date_iso, "%Y-%m-%d") else {
                            return;
                        };

                        let res = match mark_label.as_str() {
                            "Н/Б" => ctrl
                                .app
                                .add_attendance(
                                    AddAttendanceInput {
                                        profile_id: profile.id,
                                        subject_id: sid,
                                        date,
                                        time: None,
                                        status: AttendanceStatus::Absent,
                                        reason: Some(topic.to_string()),
                                        comment: Some(comment.to_string()),
                                    },
                                    RecordSource::Gui,
                                    None,
                                )
                                .map(|_| ()),
                            "Уваж." => ctrl
                                .app
                                .add_attendance(
                                    AddAttendanceInput {
                                        profile_id: profile.id,
                                        subject_id: sid,
                                        date,
                                        time: None,
                                        status: AttendanceStatus::Excused,
                                        reason: Some(topic.to_string()),
                                        comment: Some(comment.to_string()),
                                    },
                                    RecordSource::Gui,
                                    None,
                                )
                                .map(|_| ()),
                            other => {
                                let code = match other {
                                    "5" => "grade_5",
                                    "4" => "grade_4",
                                    "3" => "grade_3",
                                    "2" => "grade_2",
                                    "1" => "grade_1",
                                    "Зач." => "pass",
                                    "Н/З" => "fail",
                                    c => c,
                                };
                                let weight =
                                    Decimal::from_str(weight_str.trim()).unwrap_or(Decimal::ONE);
                                let wt = WorkType::all()
                                    .get(wt_idx as usize)
                                    .copied()
                                    .unwrap_or(WorkType::ClassAnswer);
                                ctrl.app
                                    .add_grade(
                                        AddGradeInput {
                                            profile_id: profile.id,
                                            subject_id: sid,
                                            date,
                                            time: None,
                                            mark_type_id: None,
                                            mark_code: Some(code.into()),
                                            numeric_grade: None,
                                            weight: Some(weight),
                                            work_type: Some(wt),
                                            topic: Some(topic.to_string()),
                                            comment: Some(comment.to_string()),
                                            teacher: None,
                                        },
                                        RecordSource::Gui,
                                        None,
                                    )
                                    .map(|_| ())
                            }
                        };

                        match res {
                            Ok(()) => {
                                ctrl.populate_cell_detail(&win, &sid_str, &date_iso);
                                ctrl.refresh_journal(&win);
                                ctrl.refresh_undo_flags(&win);
                                show_toast(&win, "Запись добавлена");
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                        }
                    }
                },
            );
        }

        // Delete / Copy / Move entry from cell detail
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_delete_entry(move |id_str, is_att| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                {
                    let res = if is_att {
                        ctrl.app
                            .delete_attendance(id, RecordSource::Gui, None)
                            .map(|_| ())
                    } else {
                        ctrl.app
                            .delete_grade(id, RecordSource::Gui, None)
                            .map(|_| ())
                    };
                    if res.is_ok() {
                        let sid = win.get_journal_picker_subject_id().to_string();
                        let d = win.get_journal_picker_date_iso().to_string();
                        ctrl.populate_cell_detail(&win, &sid, &d);
                        ctrl.refresh_journal(&win);
                        ctrl.refresh_undo_flags(&win);
                        show_toast(&win, "Запись удалена");
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_copy_entry(move |id_str, target_date_str| {
                if let Some(win) = w.upgrade()
                    && let (Ok(id), Ok(td)) = (
                        Uuid::parse_str(&id_str),
                        NaiveDate::parse_from_str(target_date_str.trim(), "%Y-%m-%d"),
                    )
                {
                    match ctrl.app.copy_grade_to_date(id, td, None, RecordSource::Gui) {
                        Ok(_) => {
                            ctrl.refresh_journal(&win);
                            show_toast(&win, &format!("Оценка скопирована на {td}"));
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_move_entry(move |id_str, target_date_str| {
                if let Some(win) = w.upgrade()
                    && let (Ok(id), Ok(td)) = (
                        Uuid::parse_str(&id_str),
                        NaiveDate::parse_from_str(target_date_str.trim(), "%Y-%m-%d"),
                    )
                {
                    match ctrl.app.move_grade_to_date(id, td, None, RecordSource::Gui) {
                        Ok(_) => {
                            ctrl.refresh_journal(&win);
                            show_toast(&win, &format!("Оценка перенесена на {td}"));
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_journal_subject_open_tasks(move |sid_str| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.kanban_subject_filter = Uuid::parse_str(&sid_str).ok();
                }
                if let Some(win) = w.upgrade() {
                    win.set_active_section(2);
                    ctrl.refresh_kanban(&win);
                }
            });
        }

        // Statistics & Forecast callbacks
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_stats_select_preset(move |p| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.stats_preset = p;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_statistics(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_stats_toggle_compare(move || {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.stats_compare = !st.stats_compare;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_statistics(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_stats_run_forecast(
                move |subj_idx, add_g_str, weight_str, target_str, max_w_str| {
                    if let Some(win) = w.upgrade() {
                        ctrl.handle_run_forecast(
                            &win,
                            subj_idx,
                            &add_g_str,
                            &weight_str,
                            &target_str,
                            &max_w_str,
                        );
                    }
                },
            );
        }

        // Kanban callbacks
        self.bind_kanban_callbacks(win);
        // Profiles, Subjects, Periods, Search, Notifications, Settings & MCP callbacks
        self.bind_dialogs_and_settings_callbacks(win);
    }

    fn bind_kanban_callbacks(self: &Arc<Self>, win: &MainWindow) {
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_select_board(move |bid_str| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.active_board_id = Uuid::parse_str(&bid_str).ok();
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_create_board(move |name| {
                if let Some(win) = w.upgrade()
                    && let Ok(p) = ctrl.app.get_active_profile()
                {
                    match ctrl.app.create_board(
                        CreateBoardInput {
                            profile_id: p.id,
                            name: name.to_string(),
                            description: None,
                            color: None,
                            with_default_columns: Some(true),
                        },
                        RecordSource::Gui,
                        None,
                    ) {
                        Ok(b) => {
                            if let Ok(mut st) = ctrl.state.lock() {
                                st.active_board_id = Some(b.id);
                            }
                            ctrl.refresh_kanban(&win);
                            show_toast(&win, "Доска создана");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_create_column(move |name, wip_str, done| {
                if let Some(win) = w.upgrade()
                    && let Some(bid) = ctrl.active_board_id_or_first()
                {
                    let wip = wip_str.trim().parse::<u32>().ok();
                    match ctrl.app.create_column(
                        CreateColumnInput {
                            board_id: bid,
                            name: name.to_string(),
                            color: None,
                            wip_limit: wip,
                            is_completed_column: Some(done),
                        },
                        RecordSource::Gui,
                        None,
                    ) {
                        Ok(_) => {
                            ctrl.refresh_kanban(&win);
                            show_toast(&win, "Колонка добавлена");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_move_column_delta(move |cid_str, delta| {
                if let Some(win) = w.upgrade()
                    && let (Some(bid), Ok(cid)) =
                        (ctrl.active_board_id_or_first(), Uuid::parse_str(&cid_str))
                    && let Ok(cols) = ctrl.app.list_columns(bid, false)
                    && let Some(pos) = cols.iter().position(|c| c.id == cid)
                {
                    let target =
                        (pos as i32 + delta).clamp(0, cols.len().saturating_sub(1) as i32) as usize;
                    let _ = ctrl
                        .app
                        .move_column(bid, cid, target, RecordSource::Gui, None);
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_create_task(
                move |col_str, title, desc, deadline_str, prio, subj_idx, tags_str| {
                    if let Some(win) = w.upgrade()
                        && let Ok(p) = ctrl.app.get_active_profile()
                    {
                        let cid = Uuid::parse_str(&col_str).ok();
                        let bid = ctrl.active_board_id_or_first();
                        let due_at = NaiveDate::parse_from_str(deadline_str.trim(), "%Y-%m-%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(23, 59, 0))
                            .map(|ndt| {
                                chrono::DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)
                            });
                        let subject_id = if subj_idx > 0 {
                            ctrl.app
                                .list_subjects(p.id, false)
                                .ok()
                                .and_then(|s| s.get((subj_idx - 1) as usize).map(|x| x.id))
                        } else {
                            None
                        };
                        let tags: Vec<String> = tags_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        match ctrl.app.create_task(
                            CreateTaskInput {
                                profile_id: p.id,
                                board_id: bid,
                                column_id: cid,
                                subject_id,
                                title: title.to_string(),
                                description: Some(desc.to_string()),
                                priority: Some(TaskPriority::from_i32(prio)),
                                color: None,
                                start_date: None,
                                due_at,
                                notes: None,
                                tag_names: tags,
                                badge_names: vec![],
                                checklist_titles: vec![],
                                reminder_offsets: vec![],
                            },
                            RecordSource::Gui,
                            None,
                        ) {
                            Ok(_) => {
                                ctrl.refresh_kanban(&win);
                                ctrl.refresh_undo_flags(&win);
                                show_toast(&win, "Задача создана");
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                        }
                    }
                },
            );
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_move_task_to_column(move |tid_str, cid_str| {
                if let Some(win) = w.upgrade()
                    && let (Ok(tid), Ok(cid)) =
                        (Uuid::parse_str(&tid_str), Uuid::parse_str(&cid_str))
                {
                    let _ = ctrl.app.move_task(tid, cid, None, RecordSource::Gui, None);
                    ctrl.refresh_kanban(&win);
                    ctrl.refresh_undo_flags(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_move_task_step(move |tid_str, delta| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                    && let Ok(task) = ctrl.app.get_task(tid)
                    && let Ok(cols) = ctrl.app.list_columns(task.board_id, false)
                    && let Some(pos) = cols.iter().position(|c| c.id == task.column_id)
                {
                    let next =
                        (pos as i32 + delta).clamp(0, cols.len().saturating_sub(1) as i32) as usize;
                    if next != pos {
                        let _ =
                            ctrl.app
                                .move_task(tid, cols[next].id, None, RecordSource::Gui, None);
                        ctrl.refresh_kanban(&win);
                        ctrl.refresh_undo_flags(&win);
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_toggle_task_complete(move |tid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                    && let Ok(task) = ctrl.app.get_task(tid)
                {
                    if task.is_completed() {
                        let _ = ctrl.app.reopen_task(tid, None, RecordSource::Gui, None);
                    } else {
                        let _ = ctrl.app.complete_task(tid, RecordSource::Gui, None);
                    }
                    ctrl.refresh_kanban(&win);
                    ctrl.refresh_undo_flags(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_open_task_detail(move |tid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                    && let Ok(task) = ctrl.app.get_task(tid)
                {
                    win.set_kanban_detail_task_id(tid_str);
                    win.set_kanban_detail_title(task.title.into());
                    win.set_kanban_detail_desc(task.description.into());
                    win.set_kanban_detail_notes(task.notes.into());
                    win.set_kanban_detail_deadline(
                        task.due_at
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default()
                            .into(),
                    );
                    win.set_kanban_detail_priority(task.priority.as_i32());
                    win.set_kanban_detail_tags_text(
                        task.tags
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                            .into(),
                    );
                    let checklist: Vec<ChecklistRowItem> = task
                        .checklist
                        .into_iter()
                        .map(|c| ChecklistRowItem {
                            id: c.id.to_string().into(),
                            title: c.title.into(),
                            is_completed: c.is_completed,
                        })
                        .collect();
                    win.set_kanban_detail_checklist(ModelRc::new(VecModel::from(checklist)));
                    win.set_kanban_detail_open(true);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_save_task_detail(
                move |tid_str, title, desc, notes, deadline_str, prio| {
                    if let Some(win) = w.upgrade()
                        && let Ok(tid) = Uuid::parse_str(&tid_str)
                    {
                        let due_at = if deadline_str.trim().is_empty() {
                            Some(None)
                        } else {
                            NaiveDate::parse_from_str(deadline_str.trim(), "%Y-%m-%d")
                                .ok()
                                .and_then(|d| d.and_hms_opt(23, 59, 0))
                                .map(|ndt| {
                                    Some(chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                                        ndt, Utc,
                                    ))
                                })
                        };
                        let _ = ctrl.app.update_task(
                            tid,
                            UpdateTaskInput {
                                title: Some(title.to_string()),
                                description: Some(desc.to_string()),
                                notes: Some(notes.to_string()),
                                due_at,
                                priority: Some(TaskPriority::from_i32(prio)),
                                ..Default::default()
                            },
                            RecordSource::Gui,
                            None,
                        );
                        ctrl.refresh_kanban(&win);
                        show_toast(&win, "Задача сохранена");
                    }
                },
            );
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_duplicate_task(move |tid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                {
                    let _ = ctrl.app.duplicate_task(tid, RecordSource::Gui);
                    ctrl.refresh_kanban(&win);
                    show_toast(&win, "Задача продублирована");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_delete_task(move |tid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                {
                    let _ = ctrl.app.delete_task(tid, RecordSource::Gui, None);
                    ctrl.refresh_kanban(&win);
                    ctrl.refresh_undo_flags(&win);
                    show_toast(&win, "Задача удалена");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_add_checklist(move |tid_str, title| {
                if let Some(win) = w.upgrade()
                    && let Ok(tid) = Uuid::parse_str(&tid_str)
                    && ctrl
                        .app
                        .add_checklist_item(tid, title.to_string(), RecordSource::Gui)
                        .is_ok()
                {
                    win.invoke_kanban_open_task_detail(tid_str);
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_toggle_checklist(move |cid_str, done| {
                if let Some(win) = w.upgrade()
                    && let Ok(cid) = Uuid::parse_str(&cid_str)
                {
                    let _ = ctrl.app.update_checklist_item(
                        cid,
                        None,
                        Some(done),
                        None,
                        RecordSource::Gui,
                    );
                    let tid = win.get_kanban_detail_task_id();
                    win.invoke_kanban_open_task_detail(tid);
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_delete_checklist(move |cid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(cid) = Uuid::parse_str(&cid_str)
                {
                    let _ = ctrl.app.delete_checklist_item(cid, RecordSource::Gui);
                    let tid = win.get_kanban_detail_task_id();
                    win.invoke_kanban_open_task_detail(tid);
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_add_tag(move |tid_str, tag_name| {
                if let Some(win) = w.upgrade()
                    && let (Ok(tid), Ok(p)) =
                        (Uuid::parse_str(&tid_str), ctrl.app.get_active_profile())
                    && let Ok(tag) =
                        ctrl.app
                            .get_or_create_tag(p.id, &tag_name, None, RecordSource::Gui)
                {
                    let _ = ctrl.app.assign_tag(tid, tag.id, RecordSource::Gui);
                    win.invoke_kanban_open_task_detail(tid_str);
                    ctrl.refresh_kanban(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_kanban_filter_changed(move |q, mode| {
                if let Ok(mut st) = ctrl.state.lock() {
                    st.kanban_filter_query = q.to_string();
                    st.kanban_filter_mode = mode;
                    st.kanban_subject_filter = None;
                }
                if let Some(win) = w.upgrade() {
                    ctrl.refresh_kanban(&win);
                }
            });
        }
    }

    fn bind_dialogs_and_settings_callbacks(self: &Arc<Self>, win: &MainWindow) {
        // Profiles
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_switch(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                    && ctrl.app.set_active_profile(id).is_ok()
                {
                    if let Ok(mut st) = ctrl.state.lock() {
                        st.active_board_id = None;
                        st.selected_period_id = None;
                    }
                    ctrl.refresh_all(&win);
                    show_toast(&win, "Профиль переключён");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_create(move |name, inst, grp, yr, col| {
                if let Some(win) = w.upgrade() {
                    match ctrl.app.create_profile(
                        CreateProfileInput {
                            name: name.to_string(),
                            icon: None,
                            color: Some(col.to_string()),
                            institution: Some(inst.to_string()),
                            group_name: Some(grp.to_string()),
                            academic_year: Some(yr.to_string()),
                            description: None,
                        },
                        RecordSource::Gui,
                        None,
                    ) {
                        Ok(p) => {
                            let _ = ctrl.app.set_active_profile(p.id);
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Профиль создан");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_duplicate(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                    && ctrl
                        .app
                        .duplicate_profile(id, None, true, RecordSource::Gui)
                        .is_ok()
                {
                    ctrl.refresh_profiles(&win);
                    show_toast(&win, "Профиль продублирован");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_archive(move |id_str, arch| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                {
                    match ctrl
                        .app
                        .set_profile_archived(id, arch, RecordSource::Gui, None)
                    {
                        Ok(_) => ctrl.refresh_profiles(&win),
                        Err(e) => show_toast(&win, &format!("{e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_request_delete(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                    && let Ok(imp) = ctrl.app.profile_delete_impact(id)
                {
                    win.set_profile_delete_warning(
                        format!(
                            "Внимание! Удаление профиля безвозвратно удалит {} предм., {} оценок, {} записей посещаемости и {} задач.",
                            imp.subjects, imp.grades, imp.attendance, imp.tasks
                        )
                        .into(),
                    );
                    win.set_profile_pending_delete_id(id_str);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_profile_confirm_delete(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                {
                    match ctrl.app.delete_profile(id, RecordSource::Gui, None) {
                        Ok(()) => {
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Профиль удалён");
                        }
                        Err(e) => show_toast(&win, &format!("{e}")),
                    }
                }
            });
        }

        // Subjects
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_subject_create(move |name, short, teacher, target_str, color| {
                if let Some(win) = w.upgrade()
                    && let Ok(p) = ctrl.app.get_active_profile()
                {
                    let target = Decimal::from_str(target_str.trim()).ok();
                    match ctrl.app.create_subject(
                        CreateSubjectInput {
                            profile_id: p.id,
                            name: name.to_string(),
                            short_name: Some(short.to_string()),
                            teacher: Some(teacher.to_string()),
                            color: Some(color.to_string()),
                            icon: None,
                            description: None,
                            academic_period_id: None,
                            target_average: target,
                        },
                        RecordSource::Gui,
                        None,
                    ) {
                        Ok(_) => {
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Предмет добавлен");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_subject_move(move |sid_str, delta| {
                if let Some(win) = w.upgrade()
                    && let (Ok(p), Ok(sid)) =
                        (ctrl.app.get_active_profile(), Uuid::parse_str(&sid_str))
                    && let Ok(mut list) = ctrl.app.list_subjects(p.id, false)
                    && let Some(pos) = list.iter().position(|s| s.id == sid)
                {
                    let target =
                        (pos as i32 + delta).clamp(0, list.len().saturating_sub(1) as i32) as usize;
                    let item = list.remove(pos);
                    list.insert(target, item);
                    let ids: Vec<Uuid> = list.into_iter().map(|s| s.id).collect();
                    let _ = ctrl
                        .app
                        .reorder_subjects(p.id, &ids, RecordSource::Gui, None);
                    ctrl.refresh_all(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_subject_archive(move |sid_str, arch| {
                if let Some(win) = w.upgrade()
                    && let Ok(sid) = Uuid::parse_str(&sid_str)
                {
                    let _ = ctrl
                        .app
                        .set_subject_archived(sid, arch, RecordSource::Gui, None);
                    ctrl.refresh_all(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_subject_request_delete(move |sid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(sid) = Uuid::parse_str(&sid_str)
                    && let Ok(imp) = ctrl.app.subject_delete_impact(sid)
                {
                    win.set_subject_delete_warning(
                        format!(
                            "Внимание! У предмета {} оценок, {} записей посещаемости и {} связанных задач. Удаление сотрёт оценки этого предмета.",
                            imp.grades, imp.attendance, imp.tasks
                        )
                        .into(),
                    );
                    win.set_subject_pending_delete_id(sid_str);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_subject_confirm_delete(move |sid_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(sid) = Uuid::parse_str(&sid_str)
                    && ctrl
                        .app
                        .delete_subject(sid, RecordSource::Gui, None)
                        .is_ok()
                {
                    ctrl.refresh_all(&win);
                    show_toast(&win, "Предмет удалён");
                }
            });
        }

        // Academic periods
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_period_create(move |name, pt_idx, start_str, end_str| {
                if let Some(win) = w.upgrade()
                    && let (Ok(p), Ok(s), Ok(e)) = (
                        ctrl.app.get_active_profile(),
                        NaiveDate::parse_from_str(start_str.trim(), "%Y-%m-%d"),
                        NaiveDate::parse_from_str(end_str.trim(), "%Y-%m-%d"),
                    )
                {
                    let pt = match pt_idx {
                        0 => PeriodType::AcademicYear,
                        1 => PeriodType::Semester,
                        2 => PeriodType::Quarter,
                        3 => PeriodType::Trimester,
                        _ => PeriodType::Custom,
                    };
                    match ctrl.app.create_academic_period(
                        CreateAcademicPeriodInput {
                            profile_id: p.id,
                            name: name.to_string(),
                            period_type: pt,
                            start_date: s,
                            end_date: e,
                        },
                        RecordSource::Gui,
                    ) {
                        Ok(_) => {
                            ctrl.refresh_periods(&win);
                            show_toast(&win, "Учебный период создан");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка: {e}")),
                    }
                }
            });
        }

        // Global search
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_search_query_changed(move |q| {
                if let Some(win) = w.upgrade()
                    && let Ok(p) = ctrl.app.get_active_profile()
                {
                    let hits = ctrl
                        .app
                        .search_global(
                            &q,
                            &SearchFilter {
                                profile_id: Some(p.id),
                                limit: 30,
                                ..Default::default()
                            },
                        )
                        .unwrap_or_default();
                    let mapped: Vec<SearchHitItem> = hits
                        .into_iter()
                        .map(|h| SearchHitItem {
                            entity_type: h.entity_type.as_str().into(),
                            entity_type_label: match h.entity_type {
                                journal_domain::SearchEntityType::Subject => "Предмет",
                                journal_domain::SearchEntityType::Grade => "Оценка",
                                journal_domain::SearchEntityType::Attendance => "Посещаемость",
                                journal_domain::SearchEntityType::Task => "Задача",
                                journal_domain::SearchEntityType::Tag => "Тег",
                                journal_domain::SearchEntityType::Badge => "Бейдж",
                            }
                            .into(),
                            entity_id: h.entity_id.to_string().into(),
                            title: h.title.into(),
                            snippet: h.snippet.into(),
                            date_text: h
                                .date
                                .map(|d| d.format("%d.%m.%Y").to_string())
                                .unwrap_or_default()
                                .into(),
                        })
                        .collect();
                    win.set_search_hits(ModelRc::new(VecModel::from(mapped)));
                }
            });
        }
        {
            let w = win.as_weak();
            win.on_search_hit_selected(move |et, eid| {
                if let Some(win) = w.upgrade() {
                    if et == "task" {
                        win.set_active_section(2);
                        win.invoke_kanban_open_task_detail(eid);
                    } else {
                        win.set_active_section(0);
                    }
                }
            });
        }

        // Notifications
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_notif_mark_all_read(move || {
                if let Some(win) = w.upgrade() {
                    let _ = ctrl.app.mark_all_notifications_read();
                    ctrl.refresh_notifications(&win);
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_notif_dismiss(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                {
                    let _ = ctrl.app.dismiss_notification(id);
                    ctrl.refresh_notifications(&win);
                }
            });
        }

        // Settings Persistence
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_save_appearance(
                move |theme_idx, lang_idx, accent, compact, reduced, scale_pct| {
                    if let Some(win) = w.upgrade()
                        && let Ok(mut s) = ctrl.app.get_settings()
                    {
                        s.theme_mode = match theme_idx {
                            0 => ThemeMode::Light,
                            1 => ThemeMode::Dark,
                            _ => ThemeMode::System,
                        };
                        s.language = if lang_idx == 1 {
                            Language::En
                        } else {
                            Language::Ru
                        };
                        s.accent_color = accent.to_string();
                        s.compact_mode = compact;
                        s.reduced_motion = reduced;
                        s.ui_scale = (scale_pct as f32 / 100.0).clamp(0.85, 1.4);
                        if ctrl.app.update_settings(&s, RecordSource::Gui).is_ok() {
                            ctrl.apply_settings_and_theme(&win);
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Настройки внешнего вида сохранены");
                        }
                    }
                },
            );
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_save_journal_prefs(move |mode_idx, prec_idx, week_idx, wk, hol| {
                if let Some(win) = w.upgrade()
                    && let Ok(mut s) = ctrl.app.get_settings()
                {
                    s.average_mode = if mode_idx == 1 {
                        AverageMode::Weighted
                    } else {
                        AverageMode::Arithmetic
                    };
                    s.average_precision = match prec_idx {
                        0 => AveragePrecision::One,
                        2 => AveragePrecision::Three,
                        _ => AveragePrecision::Two,
                    };
                    s.week_start = if week_idx == 1 {
                        WeekStart::Sunday
                    } else {
                        WeekStart::Monday
                    };
                    s.highlight_weekends = wk;
                    s.highlight_holidays = hol;
                    if ctrl.app.update_settings(&s, RecordSource::Gui).is_ok() {
                        ctrl.refresh_all(&win);
                        show_toast(&win, "Настройки журнала сохранены");
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_save_kanban_prefs(move |rem, os_n| {
                if let Some(win) = w.upgrade()
                    && let Ok(mut s) = ctrl.app.get_settings()
                {
                    s.reminders_enabled = rem;
                    s.os_notifications_enabled = os_n;
                    if ctrl.app.update_settings(&s, RecordSource::Gui).is_ok() {
                        show_toast(&win, "Настройки уведомлений сохранены");
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_save_mcp_prefs(move |port_str, autostart, perm_idx, destructive| {
                if let Some(win) = w.upgrade()
                    && let Ok(mut s) = ctrl.app.get_settings()
                {
                    if let Ok(port) = port_str.trim().parse::<u16>()
                        && port >= 1024
                    {
                        s.mcp_http_port = port;
                    }
                    s.mcp_http_autostart = autostart;
                    s.mcp_permission_mode = if perm_idx == 0 {
                        McpPermissionMode::ReadOnly
                    } else {
                        McpPermissionMode::ReadWrite
                    };
                    s.mcp_allow_destructive = destructive;
                    if ctrl.app.update_settings(&s, RecordSource::Gui).is_ok() {
                        ctrl.refresh_mcp_panel(&win);
                        show_toast(&win, "Права и параметры MCP сохранены");
                    }
                }
            });
        }

        // MCP Server start/stop/restart/rotate/undo
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_mcp_start_http(move || {
                if let Some(win) = w.upgrade()
                    && let (Some(rt), Ok(s)) = (ctrl.rt_handle.as_ref(), ctrl.app.get_settings())
                {
                    match ctrl.mcp.start(rt, s.mcp_http_port) {
                        Ok(()) => {
                            ctrl.refresh_mcp_panel(&win);
                            show_toast(
                                &win,
                                &format!("MCP HTTP запущен на 127.0.0.1:{}", s.mcp_http_port),
                            );
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка запуска MCP: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_mcp_stop_http(move || {
                if let Some(win) = w.upgrade() {
                    ctrl.mcp.stop();
                    ctrl.refresh_mcp_panel(&win);
                    show_toast(&win, "Встроенный MCP HTTP остановлен");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_mcp_restart_http(move || {
                if let Some(win) = w.upgrade()
                    && let (Some(rt), Ok(s)) = (ctrl.rt_handle.as_ref(), ctrl.app.get_settings())
                {
                    let _ = ctrl.mcp.restart(rt, s.mcp_http_port);
                    ctrl.refresh_mcp_panel(&win);
                    show_toast(&win, "MCP HTTP перезапущен");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_mcp_rotate_token(move || {
                if let Some(win) = w.upgrade()
                    && ctrl.app.tokens.rotate().is_ok()
                {
                    ctrl.refresh_mcp_panel(&win);
                    show_toast(&win, "Новый Bearer-токен сгенерирован");
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_mcp_undo_activity(move |id_str| {
                if let Some(win) = w.upgrade()
                    && let Ok(id) = Uuid::parse_str(&id_str)
                {
                    match ctrl.app.undo_activity_entry(id) {
                        Ok(true) => {
                            ctrl.refresh_all(&win);
                            show_toast(&win, "Действие AI отменено");
                        }
                        Ok(false) => {}
                        Err(e) => show_toast(&win, &format!("Ошибка отмены: {e}")),
                    }
                }
            });
        }

        // Backup / Restore / Export / Import
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_trigger_backup(move || {
                if let Some(win) = w.upgrade() {
                    match ctrl.app.create_backup_archive(None) {
                        Ok(info) => {
                            win.set_backup_status_text(
                                format!(
                                    "Резервная копия создана: {} ({} байт)",
                                    info.path.display(),
                                    info.size_bytes
                                )
                                .into(),
                            );
                            show_toast(&win, "Резервная копия создана");
                        }
                        Err(e) => show_toast(&win, &format!("Ошибка бэкапа: {e}")),
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_trigger_restore(move || {
                if let Some(win) = w.upgrade()
                    && let Some(file) = rfd::FileDialog::new()
                        .add_filter("Oxidium Backup ZIP", &["zip"])
                        .pick_file()
                {
                    match ctrl.app.restore_from_backup_archive(&file) {
                        Ok(()) => {
                            ctrl.refresh_all(&win);
                            win.set_backup_status_text(
                                "База успешно восстановлена из архива.".into(),
                            );
                            show_toast(&win, "База восстановлена");
                        }
                        Err(e) => {
                            win.set_backup_status_text(
                                format!("Восстановление отменено (исходные данные сохранены): {e}")
                                    .into(),
                            );
                        }
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_export_data(move |fmt| {
                if let Some(win) = w.upgrade()
                    && let Ok(p) = ctrl.app.get_active_profile()
                {
                    let ext = fmt.as_str();
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("oxidium-export.{ext}"))
                        .save_file()
                    {
                        let res = match ext {
                            "json" => ctrl.app.export_profile_json(p.id, &path),
                            "csv" => ctrl.app.export_grades_csv(p.id, &path),
                            "xlsx" => ctrl.app.export_profile_xlsx(p.id, &path),
                            "md" => ctrl.app.export_profile_markdown(p.id, &path),
                            _ => Ok(()),
                        };
                        match res {
                            Ok(()) => {
                                win.set_backup_status_text(
                                    format!("Экспорт сохранён: {}", path.display()).into(),
                                );
                                show_toast(&win, "Экспорт завершён");
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка экспорта: {e}")),
                        }
                    }
                }
            });
        }
        {
            let ctrl = Arc::clone(self);
            let w = win.as_weak();
            win.on_import_data(move |fmt| {
                if let Some(win) = w.upgrade()
                    && let Ok(p) = ctrl.app.get_active_profile()
                    && let Some(path) = rfd::FileDialog::new().pick_file()
                {
                    match fmt.as_str() {
                        "json" => match ctrl.app.import_json_into_profile(p.id, &path) {
                            Ok(prev) => {
                                ctrl.refresh_all(&win);
                                win.set_backup_status_text(
                                    format!("Импортировано JSON: {} предм., {} оценок, {} задач", prev.subjects_count, prev.grades_count, prev.tasks_count).into(),
                                );
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка импорта: {e}")),
                        },
                        "csv" => match ctrl.app.import_grades_csv(p.id, &path, false) {
                            Ok(prev) => {
                                ctrl.refresh_all(&win);
                                win.set_backup_status_text(
                                    format!("Импортировано CSV: {} оценок (предупреждений: {})", prev.grades_count, prev.warnings.len()).into(),
                                );
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка импорта: {e}")),
                        },
                        "telegram" => match ctrl.app.preview_telegram_export(&path) {
                            Ok(prev) => {
                                win.set_backup_status_text(
                                    format!("Telegram «{}»: найдено {} текстовых заметок из {} сообщений. {}", prev.chat_name, prev.non_empty_notes.len(), prev.total_messages, prev.notice).into(),
                                );
                            }
                            Err(e) => show_toast(&win, &format!("Ошибка Telegram JSON: {e}")),
                        },
                        _ => {}
                    }
                }
            });
        }
    }

    fn handle_quick_mark(&self, win: &MainWindow, sid_str: &str, date_iso: &str, mark_code: &str) {
        let Ok(profile) = self.app.get_active_profile() else {
            return;
        };
        let Ok(sid) = Uuid::parse_str(sid_str) else {
            return;
        };
        let Ok(date) = NaiveDate::parse_from_str(date_iso, "%Y-%m-%d") else {
            return;
        };

        let res = match mark_code {
            "absent" => self
                .app
                .add_attendance(
                    AddAttendanceInput {
                        profile_id: profile.id,
                        subject_id: sid,
                        date,
                        time: None,
                        status: AttendanceStatus::Absent,
                        reason: None,
                        comment: None,
                    },
                    RecordSource::Gui,
                    None,
                )
                .map(|_| ()),
            "excused" => self
                .app
                .add_attendance(
                    AddAttendanceInput {
                        profile_id: profile.id,
                        subject_id: sid,
                        date,
                        time: None,
                        status: AttendanceStatus::Excused,
                        reason: None,
                        comment: None,
                    },
                    RecordSource::Gui,
                    None,
                )
                .map(|_| ()),
            code => self
                .app
                .add_grade(
                    AddGradeInput {
                        profile_id: profile.id,
                        subject_id: sid,
                        date,
                        time: None,
                        mark_type_id: None,
                        mark_code: Some(code.to_string()),
                        numeric_grade: None,
                        weight: None,
                        work_type: None,
                        topic: None,
                        comment: None,
                        teacher: None,
                    },
                    RecordSource::Gui,
                    None,
                )
                .map(|_| ()),
        };

        match res {
            Ok(()) => {
                self.refresh_journal(win);
                self.refresh_undo_flags(win);
            }
            Err(e) => show_toast(win, &format!("Ошибка: {e}")),
        }
    }

    fn handle_run_forecast(
        &self,
        win: &MainWindow,
        subj_idx: i32,
        add_g_str: &str,
        weight_str: &str,
        target_str: &str,
        max_w_str: &str,
    ) {
        let Ok(p) = self.app.get_active_profile() else {
            return;
        };
        let Ok(settings) = self.app.get_settings() else {
            return;
        };
        let sid = if subj_idx > 0 {
            self.app
                .list_subjects(p.id, false)
                .ok()
                .and_then(|s| s.get((subj_idx - 1) as usize).map(|x| x.id))
        } else {
            None
        };
        let weight = Decimal::from_str(weight_str.trim()).unwrap_or(Decimal::ONE);
        let add_g = Decimal::from_str(add_g_str.trim()).unwrap_or(Decimal::from(5));

        if let Ok(fc) = self.app.forecast_grades(
            p.id,
            sid,
            None,
            None,
            &[GradeSample::new(add_g, weight)],
            None,
        ) {
            let cur = format_average(fc.current_average, settings.average_precision);
            let proj = format_average(fc.projected_average, settings.average_precision);
            win.set_forecast_result_text(
                format!("После оценки {add_g} (вес {weight}): {cur} → {proj}  [ПРОГНОЗ]").into(),
            );
        }

        if let Ok(target) = Decimal::from_str(target_str.trim()) {
            let max_w = max_w_str.trim().parse::<u32>().ok();
            if let Ok(reach) = self.app.forecast_needed_for_target(
                p.id, sid, None, None, target, add_g, weight, max_w, None,
            ) {
                let text = if reach.already_reached {
                    format!("Целевой балл {target} уже достигнут!")
                } else if let Some(n) = reach.grades_needed {
                    let within = match (reach.max_works_checked, reach.reachable_within_limit) {
                        (Some(lim), Some(true)) => format!(" — достижимо за {lim} работ!"),
                        (Some(lim), Some(false)) => format!(" — НЕ достижимо за {lim} работ."),
                        _ => String::new(),
                    };
                    format!(
                        "Для достижения {target} нужно оценок «{add_g}» (вес {weight}): {n}{within}"
                    )
                } else {
                    format!("Цель {target} математически недостижима оценками «{add_g}».")
                };
                win.set_forecast_target_result_text(text.into());
            }
        }
    }

    pub fn apply_settings_and_theme(&self, win: &MainWindow) {
        let Ok(s) = self.app.get_settings() else {
            return;
        };
        let schemes = build_material_schemes(&s.accent_color);
        win.global::<MaterialPalette>().set_schemes(schemes);

        let mode_idx = match s.theme_mode {
            ThemeMode::Light => 0,
            ThemeMode::Dark => 1,
            ThemeMode::System => 2,
        };
        win.invoke_apply_color_scheme(mode_idx);

        let th = win.global::<OxidiumTheme>();
        th.set_compact_mode(s.compact_mode);
        th.set_reduced_motion(s.reduced_motion);
        th.set_ui_scale(s.ui_scale);
        th.set_grade_5(parse_hex_color(
            &s.mark_colors.grade_5,
            Color::from_rgb_u8(46, 125, 50),
        ));
        th.set_grade_4(parse_hex_color(
            &s.mark_colors.grade_4,
            Color::from_rgb_u8(0, 137, 123),
        ));
        th.set_grade_3(parse_hex_color(
            &s.mark_colors.grade_3,
            Color::from_rgb_u8(217, 119, 6),
        ));
        th.set_grade_2(parse_hex_color(
            &s.mark_colors.grade_2,
            Color::from_rgb_u8(234, 88, 12),
        ));
        th.set_grade_1(parse_hex_color(
            &s.mark_colors.grade_1,
            Color::from_rgb_u8(220, 38, 38),
        ));
        th.set_mark_absent(parse_hex_color(
            &s.mark_colors.absent,
            Color::from_rgb_u8(225, 29, 72),
        ));
        th.set_mark_excused(parse_hex_color(
            &s.mark_colors.excused,
            Color::from_rgb_u8(37, 99, 235),
        ));
        th.set_mark_pass(parse_hex_color(
            &s.mark_colors.pass,
            Color::from_rgb_u8(13, 148, 136),
        ));
        th.set_mark_fail(parse_hex_color(
            &s.mark_colors.fail,
            Color::from_rgb_u8(185, 28, 28),
        ));

        // Sync settings form fields
        win.set_setting_theme_idx(mode_idx);
        win.set_setting_lang_idx(if s.language == Language::En { 1 } else { 0 });
        win.set_setting_accent_hex(s.accent_color.clone().into());
        win.set_setting_compact(s.compact_mode);
        win.set_setting_reduced_motion(s.reduced_motion);
        win.set_setting_scale_percent((s.ui_scale * 100.0).round() as i32);
        win.set_setting_avg_mode_idx(if s.average_mode == AverageMode::Weighted {
            1
        } else {
            0
        });
        win.set_setting_avg_precision_idx(match s.average_precision {
            AveragePrecision::One => 0,
            AveragePrecision::Two => 1,
            AveragePrecision::Three => 2,
        });
        win.set_setting_week_start_idx(if s.week_start == WeekStart::Sunday {
            1
        } else {
            0
        });
        win.set_setting_highlight_weekends(s.highlight_weekends);
        win.set_setting_highlight_holidays(s.highlight_holidays);
        win.set_setting_reminders_enabled(s.reminders_enabled);
        win.set_setting_os_notifications(s.os_notifications_enabled);
        win.set_mcp_port_text(s.mcp_http_port.to_string().into());
        win.set_mcp_autostart(s.mcp_http_autostart);
        win.set_mcp_permission_idx(if s.mcp_permission_mode == McpPermissionMode::ReadOnly {
            0
        } else {
            1
        });
        win.set_mcp_allow_destructive(s.mcp_allow_destructive);

        // Localized strings
        let l = s.language;
        win.set_tr(UiStrings {
            nav_journal: i18n::tr(l, "nav.journal").into(),
            nav_statistics: i18n::tr(l, "nav.statistics").into(),
            nav_kanban: i18n::tr(l, "nav.kanban").into(),
            nav_profiles: i18n::tr(l, "nav.profiles").into(),
            nav_settings: i18n::tr(l, "nav.settings").into(),
            nav_mcp: i18n::tr(l, "nav.mcp").into(),
            journal_subject: i18n::tr(l, "journal.subject").into(),
            journal_average: i18n::tr(l, "journal.average").into(),
            journal_today: i18n::tr(l, "journal.today").into(),
            journal_week: i18n::tr(l, "journal.week").into(),
            journal_month: i18n::tr(l, "journal.month").into(),
            journal_semester: i18n::tr(l, "journal.semester").into(),
            journal_year: i18n::tr(l, "journal.year").into(),
            journal_add_subject: i18n::tr(l, "journal.add_subject").into(),
            journal_empty: i18n::tr(l, "journal.empty").into(),
            search_placeholder: i18n::tr(l, "search.placeholder").into(),
            stats_overall_avg: i18n::tr(l, "stats.overall_avg").into(),
            stats_total_grades: i18n::tr(l, "stats.total_grades").into(),
            stats_absences: i18n::tr(l, "stats.absences").into(),
            stats_excused: i18n::tr(l, "stats.excused").into(),
            stats_passes: i18n::tr(l, "stats.passes").into(),
            stats_completed_tasks: i18n::tr(l, "stats.completed_tasks").into(),
            stats_overdue_tasks: i18n::tr(l, "stats.overdue_tasks").into(),
            stats_forecast_badge: i18n::tr(l, "stats.forecast_badge").into(),
            kanban_new_task: i18n::tr(l, "kanban.new_task").into(),
            kanban_new_column: i18n::tr(l, "kanban.new_column").into(),
            kanban_new_board: i18n::tr(l, "kanban.new_board").into(),
            mcp_privacy_notice: i18n::tr(l, "mcp.privacy_notice").into(),
            reminders_notice: i18n::tr(l, "reminders.background_notice").into(),
        });

        win.set_db_path_text(
            self.app
                .db
                .path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| ":memory:".into())
                .into(),
        );
        win.set_about_versions_text(
            format!(
                "Oxidium: v{}\nRust Edition: 2024\nSlint UI: 1.19.1 (Material Design 3)\nSQLite (bundled + FTS5 + WAL): {}\nMCP Server SDK: rmcp 3.11 (80 Tools • 6 Resources • 7 Prompts)",
                env!("CARGO_PKG_VERSION"),
                journal_storage::Database::sqlite_version()
            )
            .into(),
        );
    }

    pub fn refresh_all(&self, win: &MainWindow) {
        self.refresh_profiles(win);
        self.refresh_periods(win);
        self.refresh_journal(win);
        self.refresh_statistics(win);
        self.refresh_kanban(win);
        self.refresh_notifications(win);
        self.refresh_mcp_panel(win);
        self.refresh_undo_flags(win);
    }

    pub fn refresh_undo_flags(&self, win: &MainWindow) {
        win.set_can_undo(self.app.undo.can_undo());
        win.set_can_redo(self.app.undo.can_redo());
    }

    pub fn refresh_profiles(&self, win: &MainWindow) {
        let Ok(active) = self.app.get_active_profile() else {
            return;
        };
        win.set_active_profile_name(active.name.clone().into());
        let sub = [active.institution.as_str(), active.group_name.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
        win.set_active_profile_sub(sub.into());
        win.set_active_profile_color(parse_hex_color(
            &active.color,
            Color::from_rgb_u8(68, 94, 145),
        ));

        let profiles = self.app.list_profiles(true).unwrap_or_default();
        let items: Vec<ProfileItem> = profiles
            .into_iter()
            .map(|p| ProfileItem {
                id: p.id.to_string().into(),
                name: p.name.into(),
                institution: p.institution.into(),
                group_name: p.group_name.into(),
                academic_year: p.academic_year.into(),
                color: parse_hex_color(&p.color, Color::from_rgb_u8(68, 94, 145)),
                is_active: p.id == active.id,
                is_archived: p.is_archived,
            })
            .collect();
        win.set_profiles_list(ModelRc::new(VecModel::from(items)));

        let subjects = self.app.list_subjects(active.id, true).unwrap_or_default();
        let s_items: Vec<SubjectItem> = subjects
            .into_iter()
            .map(|s| SubjectItem {
                id: s.id.to_string().into(),
                name: s.name.into(),
                short_name: s.short_name.into(),
                teacher: s.teacher.into(),
                color: parse_hex_color(&s.color, Color::from_rgb_u8(68, 94, 145)),
                target_average: s
                    .target_average
                    .map(|d| d.to_string())
                    .unwrap_or_default()
                    .into(),
                is_archived: s.is_archived,
                sort_order: s.sort_order,
            })
            .collect();
        win.set_subjects_list(ModelRc::new(VecModel::from(s_items)));
    }

    pub fn refresh_periods(&self, win: &MainWindow) {
        let Ok(active) = self.app.get_active_profile() else {
            return;
        };
        let periods = self
            .app
            .list_academic_periods(active.id, false)
            .unwrap_or_default();
        let items: Vec<PeriodItem> = periods
            .into_iter()
            .map(|p| PeriodItem {
                id: p.id.to_string().into(),
                name: p.name.into(),
                period_type: p.period_type.as_str().into(),
                start_date: p.start_date.format("%Y-%m-%d").to_string().into(),
                end_date: p.end_date.format("%Y-%m-%d").to_string().into(),
                is_archived: p.is_archived,
            })
            .collect();
        win.set_academic_periods(ModelRc::new(VecModel::from(items)));
    }

    pub fn refresh_journal(&self, win: &MainWindow) {
        let Ok(active) = self.app.get_active_profile() else {
            return;
        };
        let Ok(settings) = self.app.get_settings() else {
            return;
        };
        let today = Utc::now().date_naive();
        let (year, month, view_mode, sel_period) = self
            .state
            .lock()
            .map(|st| (st.year, st.month, st.view_mode, st.selected_period_id))
            .unwrap_or((today.year(), today.month(), CalendarViewMode::Month, None));

        let matrix = match view_mode {
            CalendarViewMode::Week => {
                let anchor =
                    NaiveDate::from_ymd_opt(year, month, today.day().min(28)).unwrap_or(today);
                let (s, e) = week_bounds(anchor, settings.week_start);
                self.app.get_journal_for_range(active.id, s, e, today)
            }
            CalendarViewMode::Semester => {
                if let Some(pid) = sel_period
                    && let Ok(periods) = self.app.list_academic_periods(active.id, false)
                    && let Some(per) = periods.into_iter().find(|p| p.id == pid)
                {
                    win.set_active_period_label(per.name.into());
                    self.app
                        .get_journal_for_range(active.id, per.start_date, per.end_date, today)
                } else {
                    self.app.get_monthly_journal(active.id, year, month, today)
                }
            }
            CalendarViewMode::AcademicYear => {
                let (s, e) = academic_year_bounds(today);
                self.app.get_journal_for_range(active.id, s, e, today)
            }
            CalendarViewMode::Month | CalendarViewMode::Custom => {
                self.app.get_monthly_journal(active.id, year, month, today)
            }
        };

        let Ok(matrix) = matrix else {
            return;
        };

        let title = match view_mode {
            CalendarViewMode::Week => format!(
                "{} – {}",
                matrix.start_date.format("%d.%m"),
                matrix.end_date.format("%d.%m.%Y")
            ),
            CalendarViewMode::AcademicYear => format!(
                "{} – {}",
                matrix.start_date.format("%m.%Y"),
                matrix.end_date.format("%m.%Y")
            ),
            _ => i18n::format_month_year(settings.language, year, month),
        };
        win.set_journal_month_title(title.into());
        win.set_journal_overall_avg(matrix.overall_average_formatted.into());
        win.set_journal_avg_mode_label(
            if settings.average_mode == AverageMode::Weighted {
                "Взвеш."
            } else {
                "Средний"
            }
            .into(),
        );

        let days: Vec<DayHeaderItem> = matrix
            .days
            .iter()
            .map(|d| DayHeaderItem {
                date_iso: d.date.format("%Y-%m-%d").to_string().into(),
                day_num: format!("{:02}", d.day).into(),
                weekday_short: i18n::weekday_short(settings.language, d.weekday).into(),
                is_today: d.is_today,
                is_weekend: settings.highlight_weekends && d.is_weekend,
                is_holiday: settings.highlight_holidays && d.holiday_name.is_some(),
                holiday_name: d.holiday_name.clone().unwrap_or_default().into(),
            })
            .collect();

        if let Ok(mut st) = self.state.lock() {
            st.visible_days = matrix.days.iter().map(|d| d.date).collect();
            st.visible_subject_ids = matrix.rows.iter().map(|r| r.subject.id).collect();
        }

        let mut ui_rows = Vec::with_capacity(matrix.rows.len());
        for (r_idx, row) in matrix.rows.into_iter().enumerate() {
            let mut ui_cells = Vec::with_capacity(matrix.days.len());
            for (d_idx, day) in matrix.days.iter().enumerate() {
                let mut badges: Vec<(String, Color)> = Vec::new();
                if let Some(cell) = row.cells_by_date.get(&day.date) {
                    for g in &cell.grades {
                        badges.push((
                            g.mark_label.clone(),
                            color_for_mark_code(&g.mark_code, &settings.mark_colors),
                        ));
                    }
                    for a in &cell.attendance {
                        let (lbl, code) = match &a.status {
                            AttendanceStatus::Absent => ("Н/Б", "absent"),
                            AttendanceStatus::Excused => ("Уваж.", "excused"),
                            AttendanceStatus::Present => ("Прис.", "present"),
                            AttendanceStatus::Custom(c) => (c.as_str(), "custom"),
                        };
                        badges.push((
                            lbl.to_string(),
                            color_for_mark_code(code, &settings.mark_colors),
                        ));
                    }
                }
                let has_entries = !badges.is_empty();
                let (p_lbl, p_col) = badges
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (String::new(), Color::from_rgb_u8(120, 120, 120)));
                let (s_lbl, s_col) = badges
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| (String::new(), Color::from_rgb_u8(120, 120, 120)));
                let extra = badges.len().saturating_sub(2) as i32;

                ui_cells.push(JournalCellItem {
                    date_iso: day.date.format("%Y-%m-%d").to_string().into(),
                    day_index: d_idx as i32,
                    is_today: day.is_today,
                    is_weekend: settings.highlight_weekends && day.is_weekend,
                    is_holiday: settings.highlight_holidays && day.holiday_name.is_some(),
                    primary_label: p_lbl.into(),
                    primary_color: p_col,
                    secondary_label: s_lbl.into(),
                    secondary_color: s_col,
                    extra_count: extra,
                    has_entries,
                    tooltip: SharedString::new(),
                });
            }

            let below_target = match (row.average, row.subject.target_average) {
                (Some(a), Some(t)) => a < t,
                _ => false,
            };

            ui_rows.push(JournalRowItem {
                subject_id: row.subject.id.to_string().into(),
                subject_name: row.subject.name.into(),
                short_name: row.subject.short_name.into(),
                teacher: row.subject.teacher.into(),
                color: parse_hex_color(&row.subject.color, Color::from_rgb_u8(68, 94, 145)),
                average_text: row.average_formatted.into(),
                target_text: row
                    .subject
                    .target_average
                    .map(|d| d.to_string())
                    .unwrap_or_default()
                    .into(),
                below_target,
                grade_count: row.numeric_grade_count as i32,
                row_index: r_idx as i32,
                cells: ModelRc::new(VecModel::from(ui_cells)),
            });
        }

        win.set_journal_days(ModelRc::new(VecModel::from(days)));
        win.set_journal_rows(ModelRc::new(VecModel::from(ui_rows)));
    }

    fn populate_cell_detail(&self, win: &MainWindow, sid_str: &str, date_iso: &str) {
        let Ok(profile) = self.app.get_active_profile() else {
            return;
        };
        let Ok(settings) = self.app.get_settings() else {
            return;
        };
        let (Ok(sid), Ok(date)) = (
            Uuid::parse_str(sid_str),
            NaiveDate::parse_from_str(date_iso, "%Y-%m-%d"),
        ) else {
            return;
        };
        win.set_journal_detail_move_date(date_iso.into());

        let mut items = Vec::new();
        if let Ok(grades) = self.app.list_grades(
            profile.id,
            Some(sid),
            Some(date),
            Some(date),
            None,
            None,
            None,
            None,
        ) {
            for g in grades {
                items.push(CellDetailEntry {
                    id: g.id.to_string().into(),
                    is_attendance: false,
                    label: g.mark_label.into(),
                    color: color_for_mark_code(&g.mark_code, &settings.mark_colors),
                    weight_text: g.weight.to_string().into(),
                    work_type_label: i18n::work_type_label(settings.language, g.work_type).into(),
                    topic: g.topic.into(),
                    comment: g.comment.into(),
                    source_label: g.source.as_str().to_uppercase().into(),
                });
            }
        }
        if let Ok(att) = self
            .app
            .list_attendance(profile.id, Some(sid), Some(date), Some(date))
        {
            for a in att {
                let (lbl, code) = match &a.status {
                    AttendanceStatus::Absent => ("Н/Б", "absent"),
                    AttendanceStatus::Excused => ("Уваж.", "excused"),
                    AttendanceStatus::Present => ("Прис.", "present"),
                    AttendanceStatus::Custom(c) => (c.as_str(), "custom"),
                };
                items.push(CellDetailEntry {
                    id: a.id.to_string().into(),
                    is_attendance: true,
                    label: lbl.into(),
                    color: color_for_mark_code(code, &settings.mark_colors),
                    weight_text: SharedString::new(),
                    work_type_label: "Посещаемость".into(),
                    topic: a.reason.into(),
                    comment: a.comment.into(),
                    source_label: a.source.as_str().to_uppercase().into(),
                });
            }
        }
        win.set_cell_detail_entries(ModelRc::new(VecModel::from(items)));
    }

    pub fn refresh_statistics(&self, win: &MainWindow) {
        let Ok(active) = self.app.get_active_profile() else {
            return;
        };
        let Ok(settings) = self.app.get_settings() else {
            return;
        };
        let (preset, compare) = self
            .state
            .lock()
            .map(|st| (st.stats_preset, st.stats_compare))
            .unwrap_or((0, false));
        let today = Utc::now().date_naive();
        let (from_date, to_date) = match preset {
            1 => {
                let (s, e) = month_bounds(today.year(), today.month()).unwrap();
                (Some(s), Some(e))
            }
            2 => {
                let (py, pm) = prev_month(today.year(), today.month());
                let (s, e) = month_bounds(py, pm).unwrap();
                (Some(s), Some(e))
            }
            3 => {
                let (s, e) = academic_year_bounds(today);
                (Some(s), Some(e))
            }
            _ => (None, None),
        };

        let Ok(rep) = self.app.get_statistics(&StatisticsFilter {
            profile_id: active.id,
            from_date,
            to_date,
            ..Default::default()
        }) else {
            return;
        };

        win.set_stats_overall_avg(rep.overall_average_formatted.clone().into());
        win.set_stats_total_grades(rep.numeric_grade_count.to_string().into());
        win.set_stats_grade_counts(
            format!(
                "5: {}  •  4: {}  •  3: {}  •  2: {}  •  1: {}",
                rep.count_5, rep.count_4, rep.count_3, rep.count_2, rep.count_1
            )
            .into(),
        );
        win.set_stats_absences(rep.absences.to_string().into());
        win.set_stats_excused(rep.excused_absences.to_string().into());
        win.set_stats_passes(rep.count_pass.to_string().into());
        win.set_stats_best_subject(
            rep.best_subject
                .as_ref()
                .map(|s| format!("{} ({})", s.subject_name, s.average_formatted))
                .unwrap_or_else(|| "—".into())
                .into(),
        );
        win.set_stats_lowest_subject(
            if rep.lowest_subjects.is_empty() {
                "—".into()
            } else {
                rep.lowest_subjects
                    .iter()
                    .map(|s| format!("{} ({})", s.subject_name, s.average_formatted))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            .into(),
        );
        win.set_stats_completed_tasks(rep.task_stats.completed.to_string().into());
        win.set_stats_overdue_tasks(rep.task_stats.overdue.to_string().into());
        win.set_stats_active_tasks(rep.task_stats.active.to_string().into());

        let att_note = if let Some(pct) = rep.attendance_summary.attendance_percent {
            format!("Посещаемость по учтённым занятиям: {:.1}%", pct)
        } else {
            i18n::tr(settings.language, "stats.attendance_no_total").to_string()
        };
        win.set_stats_attendance_note(att_note.into());

        // Period comparison
        win.set_stats_compare_enabled(compare);
        if compare {
            let (py, pm) = prev_month(today.year(), today.month());
            let (b_from, b_to) = month_bounds(py, pm).unwrap();
            let (c_from, c_to) = month_bounds(today.year(), today.month()).unwrap();
            if let Ok(cmp) = self
                .app
                .compare_periods(active.id, None, b_from, b_to, c_from, c_to)
            {
                let b_str =
                    format_average(cmp.metrics.baseline_average, settings.average_precision);
                let c_str =
                    format_average(cmp.metrics.comparison_average, settings.average_precision);
                let delta_str = cmp
                    .metrics
                    .absolute_delta
                    .map(|d| format!("{:+.2}", d))
                    .unwrap_or_else(|| "—".into());
                win.set_stats_comparison_summary(
                    format!(
                        "Сравнение ({} → {}): средний балл {} → {} (Δ {}), пропуски {:+}, выполнено задач {:+}",
                        i18n::month_name(settings.language, pm),
                        i18n::month_name(settings.language, today.month()),
                        b_str,
                        c_str,
                        delta_str,
                        cmp.absences_delta,
                        cmp.completed_tasks_delta
                    )
                    .into(),
                );
            }
        } else {
            win.set_stats_comparison_summary(SharedString::new());
        }

        // Render 2x-DPI AA charts
        let dark = win.global::<OxidiumTheme>().get_dark_mode();
        let primary_c = parse_hex_color(&settings.accent_color, Color::from_rgb_u8(68, 94, 145));
        let line_pts: Vec<f32> = rep
            .average_dynamics
            .iter()
            .filter_map(|p| p.cumulative_average.to_string().parse::<f32>().ok())
            .collect();
        win.set_stats_line_chart(render_line_chart(
            &line_pts,
            720,
            260,
            dark,
            [primary_c.red(), primary_c.green(), primary_c.blue()],
        ));

        let g5_c = color_for_mark_code("grade_5", &settings.mark_colors);
        let g4_c = color_for_mark_code("grade_4", &settings.mark_colors);
        let g3_c = color_for_mark_code("grade_3", &settings.mark_colors);
        let g2_c = color_for_mark_code("grade_2", &settings.mark_colors);
        let g1_c = color_for_mark_code("grade_1", &settings.mark_colors);
        win.set_stats_donut_chart(render_donut_chart(
            &[
                (rep.count_5, [g5_c.red(), g5_c.green(), g5_c.blue()]),
                (rep.count_4, [g4_c.red(), g4_c.green(), g4_c.blue()]),
                (rep.count_3, [g3_c.red(), g3_c.green(), g3_c.blue()]),
                (rep.count_2, [g2_c.red(), g2_c.green(), g2_c.blue()]),
                (rep.count_1, [g1_c.red(), g1_c.green(), g1_c.blue()]),
            ],
            280,
            dark,
        ));

        // Grade distribution vertical bars
        let max_g = [
            rep.count_5,
            rep.count_4,
            rep.count_3,
            rep.count_2,
            rep.count_1,
        ]
        .into_iter()
        .max()
        .unwrap_or(1)
        .max(1) as f32;
        let dist_bars = vec![
            StatBarItem {
                label: "5".into(),
                value_text: rep.count_5.to_string().into(),
                ratio: rep.count_5 as f32 / max_g,
                color: g5_c,
                extra_text: SharedString::new(),
                below_target: false,
            },
            StatBarItem {
                label: "4".into(),
                value_text: rep.count_4.to_string().into(),
                ratio: rep.count_4 as f32 / max_g,
                color: g4_c,
                extra_text: SharedString::new(),
                below_target: false,
            },
            StatBarItem {
                label: "3".into(),
                value_text: rep.count_3.to_string().into(),
                ratio: rep.count_3 as f32 / max_g,
                color: g3_c,
                extra_text: SharedString::new(),
                below_target: false,
            },
            StatBarItem {
                label: "2".into(),
                value_text: rep.count_2.to_string().into(),
                ratio: rep.count_2 as f32 / max_g,
                color: g2_c,
                extra_text: SharedString::new(),
                below_target: false,
            },
            StatBarItem {
                label: "1".into(),
                value_text: rep.count_1.to_string().into(),
                ratio: rep.count_1 as f32 / max_g,
                color: g1_c,
                extra_text: SharedString::new(),
                below_target: false,
            },
        ];
        win.set_stats_grade_dist(ModelRc::new(VecModel::from(dist_bars)));

        // Subject average horizontal bars
        let subj_bars: Vec<StatBarItem> = rep
            .subject_stats
            .iter()
            .filter(|s| s.average.is_some())
            .map(|s| {
                let ratio = s
                    .average
                    .and_then(|d| d.to_string().parse::<f32>().ok())
                    .map(|v| (v / 5.0).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                StatBarItem {
                    label: s.subject_name.clone().into(),
                    value_text: s.average_formatted.clone().into(),
                    ratio,
                    color: parse_hex_color(&s.subject_color, primary_c),
                    extra_text: format!("{} оц.", s.numeric_grade_count).into(),
                    below_target: s.below_target,
                }
            })
            .collect();
        win.set_stats_subject_avgs(ModelRc::new(VecModel::from(subj_bars)));

        // Attendance bars
        let max_abs = rep
            .attendance_summary
            .by_subject
            .iter()
            .map(|a| a.absences + a.excused)
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let att_bars: Vec<StatBarItem> = rep
            .attendance_summary
            .by_subject
            .iter()
            .map(|a| StatBarItem {
                label: a.subject_name.clone().into(),
                value_text: format!("Н/Б: {} • Уваж.: {}", a.absences, a.excused).into(),
                ratio: (a.absences + a.excused) as f32 / max_abs,
                color: color_for_mark_code("absent", &settings.mark_colors),
                extra_text: SharedString::new(),
                below_target: false,
            })
            .collect();
        win.set_stats_attendance_bars(ModelRc::new(VecModel::from(att_bars)));

        // Activity heatmap (last 35 days)
        let heat_map: HashMap<NaiveDate, usize> = rep
            .activity_heatmap
            .into_iter()
            .map(|h| (h.date, h.total_activity))
            .collect();
        let mut heat_cells = Vec::with_capacity(35);
        for offset in (0..35).rev() {
            let d = today - Duration::days(offset);
            let cnt = heat_map.get(&d).copied().unwrap_or(0);
            let lvl = match cnt {
                0 => 0,
                1 => 1,
                2 => 2,
                3..=4 => 3,
                _ => 4,
            };
            heat_cells.push(HeatmapDayItem {
                date_iso: d.format("%Y-%m-%d").to_string().into(),
                day_label: format!("{}", d.day()).into(),
                level: lvl,
                tooltip: format!("{}: {}", d.format("%d.%m"), cnt).into(),
            });
        }
        win.set_stats_heatmap(ModelRc::new(VecModel::from(heat_cells)));

        // Task dynamics
        let dyn_bars: Vec<StatBarItem> = rep
            .task_stats
            .completed_by_month
            .into_iter()
            .map(|pt| StatBarItem {
                label: pt.period_label.into(),
                value_text: pt.completed_count.to_string().into(),
                ratio: 1.0,
                color: primary_c,
                extra_text: SharedString::new(),
                below_target: false,
            })
            .collect();
        win.set_stats_task_dynamics(ModelRc::new(VecModel::from(dyn_bars)));
    }

    fn active_board_id_or_first(&self) -> Option<Uuid> {
        let active_p = self.app.get_active_profile().ok()?;
        let boards = self.app.list_boards(active_p.id, false).ok()?;
        let saved = self.state.lock().ok().and_then(|st| st.active_board_id);
        if let Some(id) = saved
            && boards.iter().any(|b| b.id == id)
        {
            return Some(id);
        }
        boards.first().map(|b| b.id)
    }

    pub fn refresh_kanban(&self, win: &MainWindow) {
        let Ok(active_p) = self.app.get_active_profile() else {
            return;
        };
        let Ok(settings) = self.app.get_settings() else {
            return;
        };
        let boards = self.app.list_boards(active_p.id, false).unwrap_or_default();
        let active_bid = self.active_board_id_or_first();
        if let Ok(mut st) = self.state.lock() {
            st.active_board_id = active_bid;
        }

        let b_items: Vec<BoardItem> = boards
            .iter()
            .map(|b| BoardItem {
                id: b.id.to_string().into(),
                name: b.name.clone().into(),
            })
            .collect();
        win.set_kanban_boards(ModelRc::new(VecModel::from(b_items)));
        win.set_kanban_active_board_id(
            active_bid.map(|u| u.to_string()).unwrap_or_default().into(),
        );

        let Some(bid) = active_bid else {
            win.set_kanban_columns(ModelRc::new(
                VecModel::from(Vec::<KanbanColumnModel>::new()),
            ));
            return;
        };

        let subjects_map: HashMap<Uuid, (String, Color)> = self
            .app
            .list_subjects(active_p.id, true)
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                (
                    s.id,
                    (
                        s.name,
                        parse_hex_color(&s.color, Color::from_rgb_u8(68, 94, 145)),
                    ),
                )
            })
            .collect();

        let (f_query, f_mode, f_subj) = self
            .state
            .lock()
            .map(|st| {
                (
                    st.kanban_filter_query.to_lowercase(),
                    st.kanban_filter_mode,
                    st.kanban_subject_filter,
                )
            })
            .unwrap_or_default();

        let cols = self.app.list_columns(bid, false).unwrap_or_default();
        let all_tasks = self
            .app
            .list_tasks(active_p.id, Some(bid), None, f_subj, false)
            .unwrap_or_default();
        let now = Utc::now();

        let mut ui_cols = Vec::with_capacity(cols.len());
        for col in cols {
            let col_tasks: Vec<KanbanTaskCard> = all_tasks
                .iter()
                .filter(|t| t.column_id == col.id)
                .filter(|t| match f_mode {
                    1 => t.is_overdue(now),
                    2 => matches!(t.priority, TaskPriority::High | TaskPriority::Critical),
                    _ => true,
                })
                .filter(|t| {
                    f_query.is_empty()
                        || t.title.to_lowercase().contains(&f_query)
                        || t.description.to_lowercase().contains(&f_query)
                })
                .map(|t| {
                    let (sname, scol) = t
                        .subject_id
                        .and_then(|id| subjects_map.get(&id).cloned())
                        .unwrap_or_else(|| (String::new(), Color::from_rgb_u8(100, 116, 139)));
                    let (done_c, total_c) = t.checklist_progress();
                    let (cl_text, cl_ratio) = if total_c > 0 {
                        (
                            format!("{done_c}/{total_c}"),
                            done_c as f32 / total_c as f32,
                        )
                    } else {
                        (String::new(), 0.0)
                    };
                    let prio_color = match t.priority {
                        TaskPriority::Critical => Color::from_rgb_u8(220, 38, 38),
                        TaskPriority::High => Color::from_rgb_u8(234, 88, 12),
                        TaskPriority::Medium => Color::from_rgb_u8(217, 119, 6),
                        TaskPriority::Low => Color::from_rgb_u8(37, 99, 235),
                        TaskPriority::None => Color::from_rgb_u8(100, 116, 139),
                    };
                    KanbanTaskCard {
                        id: t.id.to_string().into(),
                        column_id: col.id.to_string().into(),
                        title: t.title.clone().into(),
                        subject_name: sname.into(),
                        subject_color: scol,
                        priority_level: t.priority.as_i32(),
                        priority_label: i18n::priority_label(settings.language, t.priority).into(),
                        priority_color: prio_color,
                        deadline_text: t
                            .due_at
                            .map(|d| d.format("%d.%m.%Y").to_string())
                            .unwrap_or_default()
                            .into(),
                        is_overdue: t.is_overdue(now),
                        is_completed: t.is_completed(),
                        tags_text: t
                            .tags
                            .iter()
                            .map(|x| x.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" #")
                            .into(),
                        badges_text: t
                            .badges
                            .iter()
                            .map(|x| x.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" • ")
                            .into(),
                        checklist_text: cl_text.into(),
                        checklist_ratio: cl_ratio,
                        linked_grades_count: t.linked_grade_ids.len() as i32,
                        has_description: !t.description.is_empty(),
                    }
                })
                .collect();

            let count = col_tasks.len() as i32;
            let wip = col.wip_limit.unwrap_or(0) as i32;
            let over_wip = wip > 0 && count > wip;
            ui_cols.push(KanbanColumnModel {
                id: col.id.to_string().into(),
                name: col.name.into(),
                color: parse_hex_color(&col.color, Color::from_rgb_u8(100, 116, 139)),
                wip_limit: wip,
                task_count: count,
                over_wip,
                is_collapsed: col.is_collapsed,
                is_completed_column: col.is_completed_column,
                tasks: ModelRc::new(VecModel::from(col_tasks)),
            });
        }
        win.set_kanban_columns(ModelRc::new(VecModel::from(ui_cols)));
    }

    pub fn refresh_notifications(&self, win: &MainWindow) {
        let items = self.app.list_notifications(false, 40).unwrap_or_default();
        let unread = items.iter().filter(|n| !n.is_read).count() as i32;
        win.set_unread_notifications(unread);
        let mapped: Vec<NotificationRowItem> = items
            .into_iter()
            .map(|n| NotificationRowItem {
                id: n.id.to_string().into(),
                title: n.title.into(),
                body: n.body.into(),
                time_text: n.created_at.format("%d.%m %H:%M").to_string().into(),
                is_read: n.is_read,
                is_overdue: n.kind == journal_domain::NotificationKind::TaskOverdue,
            })
            .collect();
        win.set_notifications_list(ModelRc::new(VecModel::from(mapped)));
    }

    pub fn refresh_mcp_panel(&self, win: &MainWindow) {
        let snap = self.mcp.metrics.snapshot();
        let running = snap.status == McpServerStatus::Running;
        win.set_mcp_http_running(running);
        win.set_mcp_status_text(
            match snap.status {
                McpServerStatus::Running => "Запущен (HTTP)",
                McpServerStatus::Stopped => "Остановлен",
                McpServerStatus::Error => "Ошибка",
            }
            .into(),
        );
        win.set_mcp_endpoint(snap.endpoint.into());
        win.set_mcp_requests_text(snap.total_requests.to_string().into());
        win.set_mcp_errors_text(snap.total_errors.to_string().into());
        win.set_mcp_last_request_text(
            snap.last_request_at
                .map(|t| t.format("%H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "—".into())
                .into(),
        );
        win.set_mcp_token_preview(
            self.app
                .tokens
                .masked_preview()
                .unwrap_or_else(|| "будет создан при запуске HTTP".into())
                .into(),
        );
        win.set_mcp_stdio_config_preview(
            r#"{ "mcpServers": { "oxidium": { "command": "student-journal-mcp", "args": [] } } }"#
                .into(),
        );

        let logs = self
            .app
            .list_activity_log(None, None, 25)
            .unwrap_or_default();
        let rows: Vec<ActivityRowItem> = logs
            .into_iter()
            .map(|l| ActivityRowItem {
                id: l.id.to_string().into(),
                time_text: l.timestamp.format("%H:%M").to_string().into(),
                source_label: l.source.as_str().to_uppercase().into(),
                summary: l.summary.into(),
                can_undo: l.undo_payload.is_some(),
                undone: l.undone,
            })
            .collect();
        win.set_activity_log_list(ModelRc::new(VecModel::from(rows)));
    }
}

fn color_for_mark_code(code: &str, palette: &journal_domain::MarkColorPalette) -> Color {
    match code {
        "grade_5" => parse_hex_color(&palette.grade_5, Color::from_rgb_u8(46, 125, 50)),
        "grade_4" => parse_hex_color(&palette.grade_4, Color::from_rgb_u8(0, 137, 123)),
        "grade_3" => parse_hex_color(&palette.grade_3, Color::from_rgb_u8(217, 119, 6)),
        "grade_2" => parse_hex_color(&palette.grade_2, Color::from_rgb_u8(234, 88, 12)),
        "grade_1" => parse_hex_color(&palette.grade_1, Color::from_rgb_u8(220, 38, 38)),
        "absent" => parse_hex_color(&palette.absent, Color::from_rgb_u8(225, 29, 72)),
        "excused" => parse_hex_color(&palette.excused, Color::from_rgb_u8(37, 99, 235)),
        "pass" => parse_hex_color(&palette.pass, Color::from_rgb_u8(13, 148, 136)),
        "fail" => parse_hex_color(&palette.fail, Color::from_rgb_u8(185, 28, 28)),
        _ => Color::from_rgb_u8(100, 116, 139),
    }
}

fn show_toast(win: &MainWindow, msg: &str) {
    win.set_snackbar_message(msg.into());
    win.set_snackbar_visible(true);
}

/// Launches the native Oxidium Slint desktop application with a non-blocking Tokio background runtime
/// handling embedded MCP HTTP, cross-process SQLite sync polling, and deadline reminders.
pub fn run_desktop_app(app: Arc<AppServices>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("oxidium-bg")
        .build()?;
    let rt_handle = rt.handle().clone();

    let mcp = Arc::new(EmbeddedHttpManager::new(Arc::clone(&app)));
    let controller = UiController::new(Arc::clone(&app), Arc::clone(&mcp), Some(rt_handle.clone()));

    let win = MainWindow::new()?;
    controller.bind_to_window(&win);

    // Background cross-process sync watcher (400ms) + reminder ticker (30s) without blocking UI thread!
    let bg_ctrl = Arc::clone(&controller);
    let weak_win = win.as_weak();
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let bg_stop = Arc::clone(&stop_flag);
    rt_handle.spawn(async move {
        let mut sync_tick = tokio::time::interval(std::time::Duration::from_millis(400));
        let mut rem_counter = 0u32;
        while !bg_stop.load(std::sync::atomic::Ordering::Relaxed) {
            sync_tick.tick().await;
            let mut needs_refresh = bg_ctrl.app.sync.poll_external_changes().is_some();
            rem_counter += 1;
            if rem_counter >= 75 {
                rem_counter = 0;
                if let Ok(fired) = bg_ctrl.app.process_due_reminders()
                    && !fired.is_empty()
                {
                    needs_refresh = true;
                }
            }
            if needs_refresh {
                let c = Arc::clone(&bg_ctrl);
                let w = weak_win.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(win) = w.upgrade() {
                        c.refresh_all(&win);
                    }
                });
            }
        }
    });

    win.run()?;

    // Graceful shutdown: stop background watcher, stop embedded HTTP MCP, checkpoint SQLite WAL
    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    mcp.stop();
    let _ = app.db.checkpoint_wal();
    drop(rt);
    Ok(())
}

#[allow(dead_code)]
fn _keep_rc_import(_: Rc<()>) {}
