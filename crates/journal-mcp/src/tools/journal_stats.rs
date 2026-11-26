use chrono::Utc;
use journal_app::StatisticsFilter;
use journal_domain::SearchEntityType;
use journal_storage::SearchFilter;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::permissions::{AccessLevel, parse_date_arg, parse_uuid_arg, run_tool};
use crate::server::OxidiumMcpServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetJournalArgs {
    pub profile_id: String,
    pub from_date: String,
    pub to_date: String,
    pub subject_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MonthlyJournalArgs {
    pub profile_id: String,
    pub year: i32,
    pub month: u32,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchJournalArgs {
    pub profile_id: String,
    pub query: String,
    pub subject_id: Option<String>,
    pub entity_type: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DateSummaryArgs {
    pub profile_id: String,
    pub date: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StatsPeriodArgs {
    pub profile_id: String,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub subject_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ComparePeriodsArgs {
    pub profile_id: String,
    pub subject_id: Option<String>,
    pub baseline_from: String,
    pub baseline_to: String,
    pub comparison_from: String,
    pub comparison_to: String,
}

#[tool_router(router = journal_stats_router)]
impl OxidiumMcpServer {
    #[tool(description = "Get structured journal entries (subjects, grades, attendance, averages) for a date range.")]
    pub async fn get_journal(
        &self,
        Parameters(args): Parameters<GetJournalArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_journal",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let from = parse_date_arg("from_date", &args.from_date)?;
                let to = parse_date_arg("to_date", &args.to_date)?;
                let mut matrix = self
                    .app
                    .get_journal_for_range(pid, from, to, Utc::now().date_naive())
                    .map_err(|e| e.to_string())?;
                if let Some(ref sid_str) = args.subject_id {
                    let sid = parse_uuid_arg("subject_id", sid_str)?;
                    matrix.rows.retain(|r| r.subject.id == sid);
                }
                Ok(matrix)
            },
        )
    }

    #[tool(description = "Get the monthly journal matrix for a profile, year, and month.")]
    pub async fn get_monthly_journal(
        &self,
        Parameters(args): Parameters<MonthlyJournalArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_monthly_journal",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .get_monthly_journal(pid, args.year, args.month, Utc::now().date_naive())
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a compact journal summary for a period.")]
    pub async fn get_journal_summary(
        &self,
        Parameters(args): Parameters<GetJournalArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_journal_summary",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let from = parse_date_arg("from_date", &args.from_date)?;
                let to = parse_date_arg("to_date", &args.to_date)?;
                self.app
                    .get_period_summary(pid, from, to)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Search across subjects, grades, attendance, tasks, tags, and badges using FTS5.")]
    pub async fn search_journal(
        &self,
        Parameters(args): Parameters<SearchJournalArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "search_journal",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let sid = args
                    .subject_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("subject_id", s))
                    .transpose()?;
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let entity_types = match args.entity_type.as_deref() {
                    Some(et) => vec![
                        SearchEntityType::parse(et)
                            .ok_or_else(|| format!("unknown entity_type: '{et}'"))?,
                    ],
                    None => vec![],
                };
                self.app
                    .search_global(
                        &args.query,
                        &SearchFilter {
                            profile_id: Some(pid),
                            subject_id: sid,
                            entity_types,
                            from_date,
                            to_date,
                            limit: args.limit.unwrap_or(50).min(200),
                        },
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a compact daily summary of grades and attendance for an ISO 8601 date.")]
    pub async fn get_daily_summary(
        &self,
        Parameters(args): Parameters<DateSummaryArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_daily_summary",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let d = parse_date_arg("date", &args.date)?;
                self.app.get_daily_summary(pid, d).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a compact weekly summary for the week containing the specified date.")]
    pub async fn get_weekly_summary(
        &self,
        Parameters(args): Parameters<DateSummaryArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_weekly_summary",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let d = parse_date_arg("date", &args.date)?;
                self.app
                    .get_weekly_summary(pid, d)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a compact monthly summary for a profile, year, and month.")]
    pub async fn get_monthly_summary(
        &self,
        Parameters(args): Parameters<MonthlyJournalArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_monthly_summary",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .get_monthly_summary(pid, args.year, args.month)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get full overall academic statistics for a profile and optional period/subject.")]
    pub async fn get_overall_statistics(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_overall_statistics",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                self.app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get per-subject averages, grade counts, target status, and absences.")]
    pub async fn get_subject_statistics(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_subject_statistics",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let rep = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(rep.subject_stats)
            },
        )
    }

    #[tool(description = "Get counts of grades 1–5 plus pass/fail marks.")]
    pub async fn get_grade_distribution(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_grade_distribution",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let rep = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "5": rep.count_5,
                    "4": rep.count_4,
                    "3": rep.count_3,
                    "2": rep.count_2,
                    "1": rep.count_1,
                    "pass": rep.count_pass,
                    "fail": rep.count_fail,
                    "total_numeric": rep.numeric_grade_count
                }))
            },
        )
    }

    #[tool(description = "Get cumulative average grade progression over time.")]
    pub async fn get_average_history(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_average_history",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let rep = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(rep.average_dynamics)
            },
        )
    }

    #[tool(description = "Get attendance summary across subjects for a period.")]
    pub async fn get_attendance_summary(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_attendance_summary",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let rep = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(rep.attendance_summary)
            },
        )
    }

    #[tool(description = "Get Kanban task completion, active, overdue, and weekly/monthly dynamics statistics.")]
    pub async fn get_task_statistics(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_task_statistics",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let subject_ids = match args.subject_id.as_deref() {
                    Some(s) => vec![parse_uuid_arg("subject_id", s)?],
                    None => vec![],
                };
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let rep = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(rep.task_stats)
            },
        )
    }

    #[tool(description = "Compare academic performance, absences, and completed tasks across two date periods.")]
    pub async fn compare_periods(
        &self,
        Parameters(args): Parameters<ComparePeriodsArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "compare_periods",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let sid = args
                    .subject_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("subject_id", s))
                    .transpose()?;
                let b_from = parse_date_arg("baseline_from", &args.baseline_from)?;
                let b_to = parse_date_arg("baseline_to", &args.baseline_to)?;
                let c_from = parse_date_arg("comparison_from", &args.comparison_from)?;
                let c_to = parse_date_arg("comparison_to", &args.comparison_to)?;
                self.app
                    .compare_periods(pid, sid, b_from, b_to, c_from, c_to)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List all subjects whose current average is below their configured target average.")]
    pub async fn get_subjects_below_target(
        &self,
        Parameters(args): Parameters<StatsPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_subjects_below_target",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let from_date = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to_date = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                self.app
                    .get_subjects_below_target(pid, from_date, to_date)
                    .map_err(|e| e.to_string())
            },
        )
    }
}

impl OxidiumMcpServer {
    pub(crate) fn build_journal_stats_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::journal_stats_router()
    }
}
