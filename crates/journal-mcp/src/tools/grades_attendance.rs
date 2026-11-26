use journal_app::{
    AddAttendanceInput, AddGradeInput, StatisticsFilter, UpdateAttendanceInput, UpdateGradeInput,
};
use journal_domain::{
    AttendanceStatus, AverageMode, GradeSample, RecordSource, WorkType, calculate_average,
    format_average,
};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::permissions::{AccessLevel, parse_date_arg, parse_uuid_arg, run_tool};
use crate::server::OxidiumMcpServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListGradesArgs {
    pub profile_id: String,
    pub subject_id: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub work_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GradeIdArgs {
    pub grade_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddGradeArgs {
    pub profile_id: String,
    pub subject_id: String,
    #[schemars(description = "ISO 8601 date YYYY-MM-DD")]
    pub date: String,
    #[schemars(description = "Optional mark code (grade_5..grade_1, pass, fail, or custom)")]
    pub mark_type: Option<String>,
    #[schemars(description = "Optional numeric grade 1..5")]
    pub grade: Option<u8>,
    #[schemars(description = "Optional positive decimal weight (default 1, control_work 2, exam 3)")]
    pub weight: Option<String>,
    pub work_type: Option<String>,
    pub topic: Option<String>,
    pub comment: Option<String>,
    pub teacher: Option<String>,
    #[schemars(description = "Optional idempotency key to prevent duplicate grades on retries")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddGradesBatchArgs {
    pub items: Vec<AddGradeArgs>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateGradeArgs {
    pub grade_id: String,
    pub date: Option<String>,
    pub weight: Option<String>,
    pub work_type: Option<String>,
    pub topic: Option<String>,
    pub comment: Option<String>,
    pub teacher: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SubjectGradesArgs {
    pub profile_id: String,
    pub subject_id: String,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GradesByDateArgs {
    pub profile_id: String,
    pub date: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GradesByPeriodArgs {
    pub profile_id: String,
    pub from_date: String,
    pub to_date: String,
    pub subject_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AverageGradeArgs {
    pub profile_id: String,
    pub subject_id: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    #[schemars(description = "Optional mode override: 'arithmetic' or 'weighted'")]
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GradeForecastArgs {
    pub profile_id: String,
    pub subject_id: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    #[schemars(description = "Optional list of hypothetical numeric grades (e.g. [5, 5]) to simulate")]
    pub hypothetical_grades: Option<Vec<u8>>,
    #[schemars(description = "Optional weight for hypothetical or target grades (default 1)")]
    pub weight: Option<String>,
    #[schemars(description = "Optional target average to calculate how many grades are needed (e.g. '4.5')")]
    pub target_average: Option<String>,
    #[schemars(description = "Assumed future grade value for target calculation (default 5)")]
    pub assumed_grade: Option<u8>,
    #[schemars(description = "Optional maximum number of future works to check reachability")]
    pub max_works: Option<u32>,
    #[schemars(description = "Optional mode override: 'arithmetic' or 'weighted'")]
    pub mode: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddAttendanceArgs {
    pub profile_id: String,
    pub subject_id: String,
    pub date: String,
    #[schemars(description = "absent, excused, present, or custom:<code>")]
    pub status: String,
    pub reason: Option<String>,
    pub comment: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateAttendanceArgs {
    pub attendance_id: String,
    pub date: Option<String>,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AttendanceIdArgs {
    pub attendance_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListAttendanceArgs {
    pub profile_id: String,
    pub subject_id: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
}

fn parse_mode(m: Option<&str>) -> Result<Option<AverageMode>, String> {
    match m.map(|s| s.trim().to_lowercase()).as_deref() {
        None | Some("") => Ok(None),
        Some("arithmetic") => Ok(Some(AverageMode::Arithmetic)),
        Some("weighted") => Ok(Some(AverageMode::Weighted)),
        Some(other) => Err(format!("invalid mode '{other}'; expected 'arithmetic' or 'weighted'")),
    }
}

fn to_add_grade_input(args: &AddGradeArgs) -> Result<AddGradeInput, String> {
    let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
    let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
    let date = parse_date_arg("date", &args.date)?;
    let weight = args
        .weight
        .as_deref()
        .map(|s| Decimal::from_str(s.trim()).map_err(|_| format!("invalid weight decimal: '{s}'")))
        .transpose()?;
    let work_type = args
        .work_type
        .as_deref()
        .map(|s| WorkType::parse(s).ok_or_else(|| format!("unknown work_type: '{s}'")))
        .transpose()?;
    Ok(AddGradeInput {
        profile_id: pid,
        subject_id: sid,
        date,
        time: None,
        mark_type_id: None,
        mark_code: args.mark_type.clone(),
        numeric_grade: args.grade,
        weight,
        work_type,
        topic: args.topic.clone(),
        comment: args.comment.clone(),
        teacher: args.teacher.clone(),
    })
}

#[tool_router(router = grades_attendance_router)]
impl OxidiumMcpServer {
    #[tool(description = "List grades with optional subject, date range, work type, and pagination filters.")]
    pub async fn list_grades(
        &self,
        Parameters(args): Parameters<ListGradesArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_grades",
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
                let from = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let wt = args
                    .work_type
                    .as_deref()
                    .map(|s| WorkType::parse(s).ok_or_else(|| format!("unknown work_type: '{s}'")))
                    .transpose()?;
                self.app
                    .list_grades(
                        pid,
                        sid,
                        from,
                        to,
                        wt,
                        None,
                        Some(args.limit.unwrap_or(100).min(500)),
                        args.offset,
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a single grade entry by UUID.")]
    pub async fn get_grade(&self, Parameters(args): Parameters<GradeIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_grade",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("grade_id", &args.grade_id)?;
                self.app.get_grade(id).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Add a grade or pass/fail mark to a subject on an ISO 8601 date. Supports idempotency_key.")]
    pub async fn add_grade(&self, Parameters(args): Parameters<AddGradeArgs>) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "add_grade",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let input = to_add_grade_input(&args)?;
                self.app
                    .add_grade(input, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Atomically add a batch of grades inside a single transaction. Supports idempotency_key.")]
    pub async fn add_grades_batch(
        &self,
        Parameters(args): Parameters<AddGradesBatchArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "add_grades_batch",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let mut inputs = Vec::with_capacity(args.items.len());
                for item in &args.items {
                    inputs.push(to_add_grade_input(item)?);
                }
                self.app
                    .add_grades_batch(inputs, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update an existing grade entry.")]
    pub async fn update_grade(
        &self,
        Parameters(args): Parameters<UpdateGradeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_grade",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("grade_id", &args.grade_id)?;
                let date = args
                    .date
                    .as_deref()
                    .map(|s| parse_date_arg("date", s))
                    .transpose()?;
                let weight = args
                    .weight
                    .as_deref()
                    .map(|s| {
                        Decimal::from_str(s.trim()).map_err(|_| format!("invalid weight: '{s}'"))
                    })
                    .transpose()?;
                let work_type = args
                    .work_type
                    .as_deref()
                    .map(|s| WorkType::parse(s).ok_or_else(|| format!("unknown work_type: '{s}'")))
                    .transpose()?;
                self.app
                    .update_grade(
                        id,
                        UpdateGradeInput {
                            date,
                            weight,
                            work_type,
                            topic: args.topic,
                            comment: args.comment,
                            teacher: args.teacher,
                            ..Default::default()
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Delete a grade entry (requires Destructive permission enabled).")]
    pub async fn delete_grade(&self, Parameters(args): Parameters<GradeIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "delete_grade",
            AccessLevel::Destructive,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("grade_id", &args.grade_id)?;
                self.app
                    .delete_grade(id, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List all grades for a specific subject.")]
    pub async fn get_subject_grades(
        &self,
        Parameters(args): Parameters<SubjectGradesArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_subject_grades",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
                let from = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                self.app
                    .list_grades(pid, Some(sid), from, to, None, None, None, None)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get all grades recorded on a specific ISO 8601 date.")]
    pub async fn get_grades_by_date(
        &self,
        Parameters(args): Parameters<GradesByDateArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_grades_by_date",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let d = parse_date_arg("date", &args.date)?;
                self.app
                    .list_grades(pid, None, Some(d), Some(d), None, None, None, None)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get all grades within an inclusive date range.")]
    pub async fn get_grades_by_period(
        &self,
        Parameters(args): Parameters<GradesByPeriodArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_grades_by_period",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let from = parse_date_arg("from_date", &args.from_date)?;
                let to = parse_date_arg("to_date", &args.to_date)?;
                let sid = args
                    .subject_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("subject_id", s))
                    .transpose()?;
                self.app
                    .list_grades(pid, sid, Some(from), Some(to), None, None, None, None)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Calculate exact arithmetic or weighted average grade for a profile or subject.")]
    pub async fn get_average_grade(
        &self,
        Parameters(args): Parameters<AverageGradeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_average_grade",
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
                let from = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let settings = self.app.get_settings().map_err(|e| e.to_string())?;
                let mode = parse_mode(args.mode.as_deref())?.unwrap_or(settings.average_mode);
                let grades = self
                    .app
                    .list_grades(pid, sid, from, to, None, None, None, None)
                    .map_err(|e| e.to_string())?;
                let avg = calculate_average(&grades, mode);
                let formatted = format_average(avg, settings.average_precision);
                let numeric_count = grades
                    .iter()
                    .filter(|g| g.counts_in_average && g.numeric_value.is_some())
                    .count();
                Ok(serde_json::json!({
                    "average": avg,
                    "average_formatted": formatted,
                    "mode": mode,
                    "numeric_grade_count": numeric_count,
                    "total_marks": grades.len(),
                }))
            },
        )
    }

    #[tool(description = "Get grade distribution and summary statistics for a profile or subject.")]
    pub async fn get_grade_statistics(
        &self,
        Parameters(args): Parameters<AverageGradeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_grade_statistics",
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
                    "overall_average": rep.overall_average,
                    "overall_average_formatted": rep.overall_average_formatted,
                    "mode": rep.average_mode,
                    "numeric_grade_count": rep.numeric_grade_count,
                    "distribution": {
                        "5": rep.count_5,
                        "4": rep.count_4,
                        "3": rep.count_3,
                        "2": rep.count_2,
                        "1": rep.count_1,
                        "pass": rep.count_pass,
                        "fail": rep.count_fail
                    }
                }))
            },
        )
    }

    #[tool(
        description = "Forecast how hypothetical grades affect GPA OR calculate how many future grades are needed to reach a target average. Never modifies journal data."
    )]
    pub async fn calculate_grade_forecast(
        &self,
        Parameters(args): Parameters<GradeForecastArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "calculate_grade_forecast",
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
                let from = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                let mode = parse_mode(args.mode.as_deref())?;
                let weight = match args.weight.as_deref() {
                    Some(w) => Decimal::from_str(w.trim())
                        .map_err(|_| format!("invalid weight decimal: '{w}'"))?,
                    None => Decimal::ONE,
                };

                let simulation = if let Some(ref hyp) = args.hypothetical_grades {
                    let samples: Vec<GradeSample> = hyp
                        .iter()
                        .map(|&v| GradeSample::new(Decimal::from(v), weight))
                        .collect();
                    Some(
                        self.app
                            .forecast_grades(pid, sid, from, to, &samples, mode)
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };

                let target_plan = if let Some(ref t_str) = args.target_average {
                    let target = Decimal::from_str(t_str.trim())
                        .map_err(|_| format!("invalid target_average decimal: '{t_str}'"))?;
                    let assumed = Decimal::from(args.assumed_grade.unwrap_or(5));
                    Some(
                        self.app
                            .forecast_needed_for_target(
                                pid,
                                sid,
                                from,
                                to,
                                target,
                                assumed,
                                weight,
                                args.max_works,
                                mode,
                            )
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };

                Ok(serde_json::json!({
                    "is_forecast": true,
                    "written_to_journal": false,
                    "simulation": simulation,
                    "target_plan": target_plan,
                }))
            },
        )
    }

    #[tool(description = "Record an attendance event (absent, excused, present, or custom) independently of numeric grades.")]
    pub async fn add_attendance(
        &self,
        Parameters(args): Parameters<AddAttendanceArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "add_attendance",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
                let date = parse_date_arg("date", &args.date)?;
                let status = AttendanceStatus::parse(&args.status).ok_or_else(|| {
                    format!(
                        "invalid attendance status '{}'; use absent, excused, present, or custom:<code>",
                        args.status
                    )
                })?;
                self.app
                    .add_attendance(
                        AddAttendanceInput {
                            profile_id: pid,
                            subject_id: sid,
                            date,
                            time: None,
                            status,
                            reason: args.reason,
                            comment: args.comment,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update an existing attendance entry.")]
    pub async fn update_attendance(
        &self,
        Parameters(args): Parameters<UpdateAttendanceArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_attendance",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("attendance_id", &args.attendance_id)?;
                let date = args
                    .date
                    .as_deref()
                    .map(|s| parse_date_arg("date", s))
                    .transpose()?;
                let status = args
                    .status
                    .as_deref()
                    .map(|s| {
                        AttendanceStatus::parse(s)
                            .ok_or_else(|| format!("invalid attendance status: '{s}'"))
                    })
                    .transpose()?;
                self.app
                    .update_attendance(
                        id,
                        UpdateAttendanceInput {
                            date,
                            time: None,
                            status,
                            reason: args.reason,
                            comment: args.comment,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Delete an attendance entry (requires Destructive permission enabled).")]
    pub async fn delete_attendance(
        &self,
        Parameters(args): Parameters<AttendanceIdArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "delete_attendance",
            AccessLevel::Destructive,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("attendance_id", &args.attendance_id)?;
                self.app
                    .delete_attendance(id, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List attendance records for a profile with optional subject and date range filters.")]
    pub async fn list_attendance(
        &self,
        Parameters(args): Parameters<ListAttendanceArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_attendance",
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
                let from = args
                    .from_date
                    .as_deref()
                    .map(|s| parse_date_arg("from_date", s))
                    .transpose()?;
                let to = args
                    .to_date
                    .as_deref()
                    .map(|s| parse_date_arg("to_date", s))
                    .transpose()?;
                self.app
                    .list_attendance(pid, sid, from, to)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get attendance summary statistics (never fabricates attendance % from empty days).")]
    pub async fn get_attendance_statistics(
        &self,
        Parameters(args): Parameters<ListAttendanceArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_attendance_statistics",
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
                let report = self
                    .app
                    .get_statistics(&StatisticsFilter {
                        profile_id: pid,
                        from_date,
                        to_date,
                        subject_ids,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                Ok(report.attendance_summary)
            },
        )
    }
}

impl OxidiumMcpServer {
    pub(crate) fn build_grades_attendance_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::grades_attendance_router()
    }
}
