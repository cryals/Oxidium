use chrono::{Datelike, NaiveDate, Utc};
use journal_domain::{
    AttendanceStatus, AverageMode, ForecastResult, GradeSample, PeriodMetricDelta,
    TargetReachability, WorkType, calculate_average_from_samples, compare_period_metrics,
    extract_countable_samples, forecast_after_grades, format_average, grades_needed_for_target,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

use crate::error::AppResult;
use crate::services::AppServices;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatisticsFilter {
    pub profile_id: Uuid,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    pub subject_ids: Vec<Uuid>,
    pub work_type: Option<WorkType>,
    pub mark_type_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectStatItem {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub subject_color: String,
    pub average: Option<Decimal>,
    pub average_formatted: String,
    pub target_average: Option<Decimal>,
    pub below_target: bool,
    pub numeric_grade_count: usize,
    pub total_mark_count: usize,
    pub absences: usize,
    pub excused_absences: usize,
    pub present_records: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AverageTrendPoint {
    pub date: NaiveDate,
    pub cumulative_average: Decimal,
    pub grade_count_so_far: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityHeatmapDay {
    pub date: NaiveDate,
    pub grades_count: usize,
    pub attendance_count: usize,
    pub tasks_count: usize,
    pub total_activity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPeriodPoint {
    pub period_label: String,
    pub completed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendanceSummary {
    pub absences: usize,
    pub excused_absences: usize,
    pub present_records: usize,
    /// `Some(pct)` ONLY when the user has explicitly recorded `present` sessions alongside absences;
    /// otherwise `None` so empty days are NEVER fabricated as attended classes.
    pub attendance_percent: Option<Decimal>,
    pub has_sufficient_session_data: bool,
    pub by_subject: Vec<SubjectAttendanceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectAttendanceItem {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub absences: usize,
    pub excused: usize,
    pub present: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatistics {
    pub total: usize,
    pub completed: usize,
    pub active: usize,
    pub overdue: usize,
    pub completed_by_week: Vec<TaskPeriodPoint>,
    pub completed_by_month: Vec<TaskPeriodPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsReport {
    pub profile_id: Uuid,
    pub average_mode: AverageMode,
    pub overall_average: Option<Decimal>,
    pub overall_average_formatted: String,
    pub total_marks: usize,
    pub numeric_grade_count: usize,
    pub count_5: usize,
    pub count_4: usize,
    pub count_3: usize,
    pub count_2: usize,
    pub count_1: usize,
    pub count_pass: usize,
    pub count_fail: usize,
    pub absences: usize,
    pub excused_absences: usize,
    pub best_subject: Option<SubjectStatItem>,
    pub lowest_subjects: Vec<SubjectStatItem>,
    pub subject_stats: Vec<SubjectStatItem>,
    pub average_dynamics: Vec<AverageTrendPoint>,
    pub attendance_summary: AttendanceSummary,
    pub activity_heatmap: Vec<ActivityHeatmapDay>,
    pub task_stats: TaskStatistics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodComparisonReport {
    pub baseline_from: NaiveDate,
    pub baseline_to: NaiveDate,
    pub comparison_from: NaiveDate,
    pub comparison_to: NaiveDate,
    pub metrics: PeriodMetricDelta,
    pub baseline_absences: usize,
    pub comparison_absences: usize,
    pub absences_delta: i64,
    pub baseline_completed_tasks: usize,
    pub comparison_completed_tasks: usize,
    pub completed_tasks_delta: i64,
}

impl AppServices {
    pub fn get_statistics(&self, filter: &StatisticsFilter) -> AppResult<StatisticsReport> {
        let rev = self.sync.current_revision();
        let cache_key = serde_json::to_string(filter).unwrap_or_default();
        if let Ok(cache) = self.stats_cache.lock()
            && let Some((cached_rev, report)) = cache.get(&cache_key)
            && *cached_rev == rev
        {
            return Ok(report.clone());
        }

        let settings = self.db.load_settings()?;
        let all_subjects = self.db.list_subjects(filter.profile_id, false)?;
        let subjects: Vec<_> = if filter.subject_ids.is_empty() {
            all_subjects
        } else {
            all_subjects
                .into_iter()
                .filter(|s| filter.subject_ids.contains(&s.id))
                .collect()
        };

        let raw_grades = self.db.list_grades(
            filter.profile_id,
            None,
            filter.from_date,
            filter.to_date,
            filter.work_type,
            filter.mark_type_id,
            None,
            None,
        )?;
        let grades: Vec<_> = if filter.subject_ids.is_empty() {
            raw_grades
        } else {
            raw_grades
                .into_iter()
                .filter(|g| filter.subject_ids.contains(&g.subject_id))
                .collect()
        };

        let raw_att =
            self.db
                .list_attendance(filter.profile_id, None, filter.from_date, filter.to_date)?;
        let attendance: Vec<_> = if filter.subject_ids.is_empty() {
            raw_att
        } else {
            raw_att
                .into_iter()
                .filter(|a| filter.subject_ids.contains(&a.subject_id))
                .collect()
        };

        let all_tasks = self
            .db
            .list_tasks(filter.profile_id, None, None, None, false)?;
        let tasks: Vec<_> = if filter.subject_ids.is_empty() {
            all_tasks
        } else {
            all_tasks
                .into_iter()
                .filter(|t| {
                    t.subject_id
                        .is_some_and(|sid| filter.subject_ids.contains(&sid))
                })
                .collect()
        };

        let countable = extract_countable_samples(&grades);
        let overall_avg = calculate_average_from_samples(&countable, settings.average_mode);
        let overall_fmt = format_average(overall_avg, settings.average_precision);

        let mut c5 = 0usize;
        let mut c4 = 0usize;
        let mut c3 = 0usize;
        let mut c2 = 0usize;
        let mut c1 = 0usize;
        let mut c_pass = 0usize;
        let mut c_fail = 0usize;
        for g in &grades {
            match g.mark_code.as_str() {
                "grade_5" => c5 += 1,
                "grade_4" => c4 += 1,
                "grade_3" => c3 += 1,
                "grade_2" => c2 += 1,
                "grade_1" => c1 += 1,
                "pass" => c_pass += 1,
                "fail" => c_fail += 1,
                _ => {
                    if g.counts_in_average
                        && let Some(v) = g.numeric_value
                    {
                        if v == Decimal::from(5) {
                            c5 += 1;
                        } else if v == Decimal::from(4) {
                            c4 += 1;
                        } else if v == Decimal::from(3) {
                            c3 += 1;
                        } else if v == Decimal::from(2) {
                            c2 += 1;
                        } else if v == Decimal::from(1) {
                            c1 += 1;
                        }
                    }
                }
            }
        }

        // Per-subject stats
        let mut grades_by_s: HashMap<Uuid, Vec<journal_domain::GradeEntry>> = HashMap::new();
        for g in &grades {
            grades_by_s.entry(g.subject_id).or_default().push(g.clone());
        }
        let mut att_by_s: HashMap<Uuid, (usize, usize, usize)> = HashMap::new();
        let mut total_abs = 0usize;
        let mut total_exc = 0usize;
        let mut total_pres = 0usize;
        for a in &attendance {
            let e = att_by_s.entry(a.subject_id).or_insert((0, 0, 0));
            match a.status {
                AttendanceStatus::Absent => {
                    e.0 += 1;
                    total_abs += 1;
                }
                AttendanceStatus::Excused => {
                    e.1 += 1;
                    total_exc += 1;
                }
                AttendanceStatus::Present => {
                    e.2 += 1;
                    total_pres += 1;
                }
                AttendanceStatus::Custom(_) => {}
            }
        }

        let mut subject_stats = Vec::with_capacity(subjects.len());
        let mut att_by_subject = Vec::new();
        for s in &subjects {
            let sg = grades_by_s.get(&s.id).map(Vec::as_slice).unwrap_or(&[]);
            let s_samples = extract_countable_samples(sg);
            let s_avg = calculate_average_from_samples(&s_samples, settings.average_mode);
            let (abs, exc, pres) = att_by_s.get(&s.id).copied().unwrap_or((0, 0, 0));
            let below_target = match (s_avg, s.target_average) {
                (Some(a), Some(t)) => a < t,
                _ => false,
            };
            subject_stats.push(SubjectStatItem {
                subject_id: s.id,
                subject_name: s.name.clone(),
                subject_color: s.color.clone(),
                average: s_avg,
                average_formatted: format_average(s_avg, settings.average_precision),
                target_average: s.target_average,
                below_target,
                numeric_grade_count: s_samples.len(),
                total_mark_count: sg.len(),
                absences: abs,
                excused_absences: exc,
                present_records: pres,
            });
            if abs > 0 || exc > 0 || pres > 0 {
                att_by_subject.push(SubjectAttendanceItem {
                    subject_id: s.id,
                    subject_name: s.name.clone(),
                    absences: abs,
                    excused: exc,
                    present: pres,
                });
            }
        }

        let mut ranked: Vec<SubjectStatItem> = subject_stats
            .iter()
            .filter(|s| s.average.is_some())
            .cloned()
            .collect();
        ranked.sort_by_key(|a| std::cmp::Reverse(a.average));
        let best_subject = ranked.first().cloned();
        let mut lowest_subjects = ranked;
        lowest_subjects.reverse();
        lowest_subjects.truncate(3);

        // Cumulative average dynamics by date
        let mut grades_by_date: BTreeMap<NaiveDate, Vec<GradeSample>> = BTreeMap::new();
        for g in &grades {
            if g.counts_in_average
                && let Some(val) = g.numeric_value
            {
                grades_by_date
                    .entry(g.date)
                    .or_default()
                    .push(GradeSample::new(val, g.weight));
            }
        }
        let mut running_samples: Vec<GradeSample> = Vec::new();
        let mut average_dynamics = Vec::with_capacity(grades_by_date.len());
        for (date, day_samples) in grades_by_date {
            running_samples.extend(day_samples);
            if let Some(cum_avg) =
                calculate_average_from_samples(&running_samples, settings.average_mode)
            {
                average_dynamics.push(AverageTrendPoint {
                    date,
                    cumulative_average: cum_avg,
                    grade_count_so_far: running_samples.len(),
                });
            }
        }

        // Honest attendance percentage: ONLY if `present` sessions are actively tracked
        let has_sufficient = total_pres > 0;
        let attendance_percent = if has_sufficient {
            let denom = Decimal::from(total_pres + total_abs + total_exc);
            if denom.is_zero() {
                None
            } else {
                Some((Decimal::from(total_pres) / denom) * Decimal::from(100))
            }
        } else {
            None
        };

        // Calendar activity heatmap (grades + attendance + tasks with due/completed dates)
        let mut heat: BTreeMap<NaiveDate, (usize, usize, usize)> = BTreeMap::new();
        for g in &grades {
            heat.entry(g.date).or_insert((0, 0, 0)).0 += 1;
        }
        for a in &attendance {
            heat.entry(a.date).or_insert((0, 0, 0)).1 += 1;
        }
        let now = Utc::now();
        let mut completed_tasks = 0usize;
        let mut overdue_tasks = 0usize;
        let mut active_tasks = 0usize;
        let mut by_week: BTreeMap<String, usize> = BTreeMap::new();
        let mut by_month: BTreeMap<String, usize> = BTreeMap::new();

        for t in &tasks {
            if let Some(done_at) = t.completed_at {
                completed_tasks += 1;
                let d = done_at.date_naive();
                if filter.from_date.is_none_or(|f| d >= f)
                    && filter.to_date.is_none_or(|to| d <= to)
                {
                    heat.entry(d).or_insert((0, 0, 0)).2 += 1;
                }
                let iso = d.iso_week();
                *by_week
                    .entry(format!("{}-W{:02}", iso.year(), iso.week()))
                    .or_insert(0) += 1;
                *by_month
                    .entry(format!("{}-{:02}", d.year(), d.month()))
                    .or_insert(0) += 1;
            } else if t.is_overdue(now) {
                overdue_tasks += 1;
            } else {
                active_tasks += 1;
            }
            if let Some(due) = t.due_at {
                let d = due.date_naive();
                if filter.from_date.is_none_or(|f| d >= f)
                    && filter.to_date.is_none_or(|to| d <= to)
                {
                    heat.entry(d).or_insert((0, 0, 0)).2 += 1;
                }
            }
        }

        let activity_heatmap = heat
            .into_iter()
            .map(|(date, (gc, ac, tc))| ActivityHeatmapDay {
                date,
                grades_count: gc,
                attendance_count: ac,
                tasks_count: tc,
                total_activity: gc + ac + tc,
            })
            .collect();

        let report = StatisticsReport {
            profile_id: filter.profile_id,
            average_mode: settings.average_mode,
            overall_average: overall_avg,
            overall_average_formatted: overall_fmt,
            total_marks: grades.len(),
            numeric_grade_count: countable.len(),
            count_5: c5,
            count_4: c4,
            count_3: c3,
            count_2: c2,
            count_1: c1,
            count_pass: c_pass,
            count_fail: c_fail,
            absences: total_abs,
            excused_absences: total_exc,
            best_subject,
            lowest_subjects,
            subject_stats,
            average_dynamics,
            attendance_summary: AttendanceSummary {
                absences: total_abs,
                excused_absences: total_exc,
                present_records: total_pres,
                attendance_percent,
                has_sufficient_session_data: has_sufficient,
                by_subject: att_by_subject,
            },
            activity_heatmap,
            task_stats: TaskStatistics {
                total: tasks.len(),
                completed: completed_tasks,
                active: active_tasks,
                overdue: overdue_tasks,
                completed_by_week: by_week
                    .into_iter()
                    .map(|(period_label, completed_count)| TaskPeriodPoint {
                        period_label,
                        completed_count,
                    })
                    .collect(),
                completed_by_month: by_month
                    .into_iter()
                    .map(|(period_label, completed_count)| TaskPeriodPoint {
                        period_label,
                        completed_count,
                    })
                    .collect(),
            },
        };

        if let Ok(mut cache) = self.stats_cache.lock() {
            if cache.len() > 64 {
                cache.clear();
            }
            cache.insert(cache_key, (rev, report.clone()));
        }

        Ok(report)
    }

    pub fn compare_periods(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        baseline_from: NaiveDate,
        baseline_to: NaiveDate,
        comparison_from: NaiveDate,
        comparison_to: NaiveDate,
    ) -> AppResult<PeriodComparisonReport> {
        journal_domain::validate_date_range(baseline_from, baseline_to)?;
        journal_domain::validate_date_range(comparison_from, comparison_to)?;
        let settings = self.db.load_settings()?;

        let g1 = self.db.list_grades(
            profile_id,
            subject_id,
            Some(baseline_from),
            Some(baseline_to),
            None,
            None,
            None,
            None,
        )?;
        let g2 = self.db.list_grades(
            profile_id,
            subject_id,
            Some(comparison_from),
            Some(comparison_to),
            None,
            None,
            None,
            None,
        )?;
        let a1 = self.db.list_attendance(
            profile_id,
            subject_id,
            Some(baseline_from),
            Some(baseline_to),
        )?;
        let a2 = self.db.list_attendance(
            profile_id,
            subject_id,
            Some(comparison_from),
            Some(comparison_to),
        )?;

        let abs1 = a1
            .iter()
            .filter(|a| a.status == AttendanceStatus::Absent)
            .count();
        let abs2 = a2
            .iter()
            .filter(|a| a.status == AttendanceStatus::Absent)
            .count();

        let tasks = self
            .db
            .list_tasks(profile_id, None, None, subject_id, false)?;
        let t1 = tasks
            .iter()
            .filter(|t| {
                t.completed_at.is_some_and(|dt| {
                    let d = dt.date_naive();
                    d >= baseline_from && d <= baseline_to
                })
            })
            .count();
        let t2 = tasks
            .iter()
            .filter(|t| {
                t.completed_at.is_some_and(|dt| {
                    let d = dt.date_naive();
                    d >= comparison_from && d <= comparison_to
                })
            })
            .count();

        let metrics = compare_period_metrics(
            &extract_countable_samples(&g1),
            &extract_countable_samples(&g2),
            settings.average_mode,
        );

        Ok(PeriodComparisonReport {
            baseline_from,
            baseline_to,
            comparison_from,
            comparison_to,
            metrics,
            baseline_absences: abs1,
            comparison_absences: abs2,
            absences_delta: abs2 as i64 - abs1 as i64,
            baseline_completed_tasks: t1,
            comparison_completed_tasks: t2,
            completed_tasks_delta: t2 as i64 - t1 as i64,
        })
    }

    pub fn get_subjects_below_target(
        &self,
        profile_id: Uuid,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
    ) -> AppResult<Vec<SubjectStatItem>> {
        let report = self.get_statistics(&StatisticsFilter {
            profile_id,
            from_date,
            to_date,
            ..Default::default()
        })?;
        Ok(report
            .subject_stats
            .into_iter()
            .filter(|s| s.below_target)
            .collect())
    }

    /// Simulates hypothetical grades (never persists to database).
    pub fn forecast_grades(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
        hypothetical: &[GradeSample],
        mode_override: Option<AverageMode>,
    ) -> AppResult<ForecastResult> {
        let settings = self.db.load_settings()?;
        let mode = mode_override.unwrap_or(settings.average_mode);
        let grades = self.db.list_grades(
            profile_id, subject_id, from_date, to_date, None, None, None, None,
        )?;
        let current = extract_countable_samples(&grades);
        Ok(forecast_after_grades(&current, hypothetical, mode))
    }

    /// Calculates how many grades of `assumed_grade` (with `assumed_weight`) are needed to reach `target_average`.
    #[allow(clippy::too_many_arguments)]
    pub fn forecast_needed_for_target(
        &self,
        profile_id: Uuid,
        subject_id: Option<Uuid>,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
        target_average: Decimal,
        assumed_grade: Decimal,
        assumed_weight: Decimal,
        max_works: Option<u32>,
        mode_override: Option<AverageMode>,
    ) -> AppResult<TargetReachability> {
        let settings = self.db.load_settings()?;
        let mode = mode_override.unwrap_or(settings.average_mode);
        let grades = self.db.list_grades(
            profile_id, subject_id, from_date, to_date, None, None, None, None,
        )?;
        let current = extract_countable_samples(&grades);
        Ok(grades_needed_for_target(
            &current,
            target_average,
            assumed_grade,
            assumed_weight,
            mode,
            max_works,
        ))
    }
}
