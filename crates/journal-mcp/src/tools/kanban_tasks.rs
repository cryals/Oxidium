use journal_app::{
    CreateBoardInput, CreateColumnInput, CreateTaskInput, UpdateBoardInput, UpdateColumnInput,
    UpdateTaskInput,
};
use journal_domain::{RecordSource, TaskPriority};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::permissions::{AccessLevel, parse_date_arg, parse_dt_arg, parse_uuid_arg, run_tool};
use crate::server::OxidiumMcpServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListBoardsArgs {
    pub profile_id: String,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BoardIdArgs {
    pub board_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateBoardArgs {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub with_default_columns: Option<bool>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateBoardArgs {
    pub board_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListColumnsArgs {
    pub board_id: String,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateColumnArgs {
    pub board_id: String,
    pub name: String,
    pub color: Option<String>,
    pub wip_limit: Option<u32>,
    pub is_completed_column: Option<bool>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateColumnArgs {
    pub column_id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub wip_limit: Option<u32>,
    pub is_collapsed: Option<bool>,
    pub is_completed_column: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MoveColumnArgs {
    pub board_id: String,
    pub column_id: String,
    pub target_index: usize,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveColumnArgs {
    pub column_id: String,
    pub archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListTasksArgs {
    pub profile_id: String,
    pub board_id: Option<String>,
    pub column_id: Option<String>,
    pub subject_id: Option<String>,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskIdArgs {
    pub task_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskArgs {
    pub profile_id: String,
    pub title: String,
    pub description: Option<String>,
    pub board_id: Option<String>,
    pub column_id: Option<String>,
    pub subject_id: Option<String>,
    #[schemars(description = "none, low, medium, high, or critical")]
    pub priority: Option<String>,
    pub color: Option<String>,
    pub start_date: Option<String>,
    #[schemars(description = "ISO 8601 deadline timestamp or YYYY-MM-DD date")]
    pub deadline: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub badges: Option<Vec<String>>,
    pub checklist: Option<Vec<String>>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateTasksBatchArgs {
    pub items: Vec<CreateTaskArgs>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskArgs {
    pub task_id: String,
    pub column_id: Option<String>,
    pub subject_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub color: Option<String>,
    pub deadline: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MoveTaskArgs {
    pub task_id: String,
    pub target_column_id: String,
    pub target_order: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReopenTaskArgs {
    pub task_id: String,
    pub target_column_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveTaskArgs {
    pub task_id: String,
    pub archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OverdueTasksArgs {
    pub profile_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpcomingTasksArgs {
    pub profile_id: String,
    #[schemars(description = "Number of days ahead (default 7)")]
    pub within_days: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TasksBySubjectArgs {
    pub profile_id: String,
    pub subject_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchTasksArgs {
    pub profile_id: String,
    pub query: String,
}

fn to_create_task_input(args: &CreateTaskArgs) -> Result<CreateTaskInput, String> {
    let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
    let bid = args
        .board_id
        .as_deref()
        .map(|s| parse_uuid_arg("board_id", s))
        .transpose()?;
    let cid = args
        .column_id
        .as_deref()
        .map(|s| parse_uuid_arg("column_id", s))
        .transpose()?;
    let sid = args
        .subject_id
        .as_deref()
        .map(|s| parse_uuid_arg("subject_id", s))
        .transpose()?;
    let priority = args
        .priority
        .as_deref()
        .map(|s| TaskPriority::parse(s).ok_or_else(|| format!("invalid priority: '{s}'")))
        .transpose()?;
    let start_date = args
        .start_date
        .as_deref()
        .map(|s| parse_date_arg("start_date", s))
        .transpose()?;
    let due_at = args
        .deadline
        .as_deref()
        .map(|s| parse_dt_arg("deadline", s))
        .transpose()?;
    Ok(CreateTaskInput {
        profile_id: pid,
        board_id: bid,
        column_id: cid,
        subject_id: sid,
        title: args.title.clone(),
        description: args.description.clone(),
        priority,
        color: args.color.clone(),
        start_date,
        due_at,
        notes: args.notes.clone(),
        tag_names: args.tags.clone().unwrap_or_default(),
        badge_names: args.badges.clone().unwrap_or_default(),
        checklist_titles: args.checklist.clone().unwrap_or_default(),
        reminder_offsets: vec![],
    })
}

#[tool_router(router = kanban_tasks_router)]
impl OxidiumMcpServer {
    #[tool(description = "List Kanban boards belonging to a profile.")]
    pub async fn list_boards(
        &self,
        Parameters(args): Parameters<ListBoardsArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_boards",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .list_boards(pid, args.include_archived.unwrap_or(false))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a Kanban board with its columns.")]
    pub async fn get_board(&self, Parameters(args): Parameters<BoardIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_board",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let bid = parse_uuid_arg("board_id", &args.board_id)?;
                let board = self.app.get_board(bid).map_err(|e| e.to_string())?;
                let columns = self
                    .app
                    .list_columns(bid, false)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "board": board, "columns": columns }))
            },
        )
    }

    #[tool(description = "Create a new Kanban board inside a profile.")]
    pub async fn create_board(
        &self,
        Parameters(args): Parameters<CreateBoardArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_board",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .create_board(
                        CreateBoardInput {
                            profile_id: pid,
                            name: args.name,
                            description: args.description,
                            color: args.color,
                            with_default_columns: args.with_default_columns,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update a Kanban board's name, description, color, or archive state.")]
    pub async fn update_board(
        &self,
        Parameters(args): Parameters<UpdateBoardArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_board",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let bid = parse_uuid_arg("board_id", &args.board_id)?;
                self.app
                    .update_board(
                        bid,
                        UpdateBoardInput {
                            name: args.name,
                            description: args.description,
                            color: args.color,
                            is_archived: args.is_archived,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List columns of a Kanban board.")]
    pub async fn list_columns(
        &self,
        Parameters(args): Parameters<ListColumnsArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_columns",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let bid = parse_uuid_arg("board_id", &args.board_id)?;
                self.app
                    .list_columns(bid, args.include_archived.unwrap_or(false))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Create a new column on a Kanban board.")]
    pub async fn create_column(
        &self,
        Parameters(args): Parameters<CreateColumnArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_column",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let bid = parse_uuid_arg("board_id", &args.board_id)?;
                self.app
                    .create_column(
                        CreateColumnInput {
                            board_id: bid,
                            name: args.name,
                            color: args.color,
                            wip_limit: args.wip_limit,
                            is_completed_column: args.is_completed_column,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Update a Kanban column's properties (name, color, WIP limit, completion flag)."
    )]
    pub async fn update_column(
        &self,
        Parameters(args): Parameters<UpdateColumnArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_column",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let cid = parse_uuid_arg("column_id", &args.column_id)?;
                self.app
                    .update_column(
                        cid,
                        UpdateColumnInput {
                            name: args.name,
                            color: args.color,
                            wip_limit: args.wip_limit.map(Some),
                            is_collapsed: args.is_collapsed,
                            is_archived: None,
                            is_completed_column: args.is_completed_column,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Move a Kanban column to a new zero-based position on its board.")]
    pub async fn move_column(
        &self,
        Parameters(args): Parameters<MoveColumnArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "move_column",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let bid = parse_uuid_arg("board_id", &args.board_id)?;
                let cid = parse_uuid_arg("column_id", &args.column_id)?;
                self.app
                    .move_column(bid, cid, args.target_index, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Archive or restore a Kanban column.")]
    pub async fn archive_column(
        &self,
        Parameters(args): Parameters<ArchiveColumnArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "archive_column",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let cid = parse_uuid_arg("column_id", &args.column_id)?;
                self.app
                    .update_column(
                        cid,
                        UpdateColumnInput {
                            is_archived: Some(args.archived.unwrap_or(true)),
                            ..Default::default()
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "List Kanban tasks with optional board, column, subject, and archive filters."
    )]
    pub async fn list_tasks(&self, Parameters(args): Parameters<ListTasksArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_tasks",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let bid = args
                    .board_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("board_id", s))
                    .transpose()?;
                let cid = args
                    .column_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("column_id", s))
                    .transpose()?;
                let sid = args
                    .subject_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("subject_id", s))
                    .transpose()?;
                self.app
                    .list_tasks(pid, bid, cid, sid, args.include_archived.unwrap_or(false))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Get full task details including tags, badges, checklist, and linked grades."
    )]
    pub async fn get_task(&self, Parameters(args): Parameters<TaskIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_task",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("task_id", &args.task_id)?;
                self.app.get_task(id).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Create a new academic Kanban task with optional subject, priority, deadline, tags, badges, and checklist."
    )]
    pub async fn create_task(
        &self,
        Parameters(args): Parameters<CreateTaskArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_task",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let input = to_create_task_input(&args)?;
                self.app
                    .create_task(input, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Atomically create multiple Kanban tasks in a single request. Supports idempotency_key."
    )]
    pub async fn create_tasks_batch(
        &self,
        Parameters(args): Parameters<CreateTasksBatchArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_tasks_batch",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let mut inputs = Vec::with_capacity(args.items.len());
                for item in &args.items {
                    inputs.push(to_create_task_input(item)?);
                }
                self.app
                    .create_tasks_batch(inputs, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Update an existing Kanban task's fields (title, description, priority, deadline, subject, notes)."
    )]
    pub async fn update_task(
        &self,
        Parameters(args): Parameters<UpdateTaskArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_task",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                let cid = args
                    .column_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("column_id", s))
                    .transpose()?;
                let sid = args
                    .subject_id
                    .as_deref()
                    .map(|s| {
                        if s.trim().is_empty() {
                            Ok(None)
                        } else {
                            parse_uuid_arg("subject_id", s).map(Some)
                        }
                    })
                    .transpose()?;
                let prio = args
                    .priority
                    .as_deref()
                    .map(|s| {
                        TaskPriority::parse(s).ok_or_else(|| format!("invalid priority: '{s}'"))
                    })
                    .transpose()?;
                let due = args
                    .deadline
                    .as_deref()
                    .map(|s| {
                        if s.trim().is_empty() {
                            Ok(None)
                        } else {
                            parse_dt_arg("deadline", s).map(Some)
                        }
                    })
                    .transpose()?;
                self.app
                    .update_task(
                        tid,
                        UpdateTaskInput {
                            column_id: cid,
                            subject_id: sid,
                            title: args.title,
                            description: args.description,
                            priority: prio,
                            color: args.color.map(Some),
                            due_at: due,
                            notes: args.notes,
                            ..Default::default()
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Move a Kanban task to another column and/or position.")]
    pub async fn move_task(&self, Parameters(args): Parameters<MoveTaskArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "move_task",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                let cid = parse_uuid_arg("target_column_id", &args.target_column_id)?;
                self.app
                    .move_task(tid, cid, args.target_order, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Mark a task as completed and move it to the board's completed column.")]
    pub async fn complete_task(&self, Parameters(args): Parameters<TaskIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "complete_task",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                self.app
                    .complete_task(tid, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Reopen a completed task and move it back to an active column.")]
    pub async fn reopen_task(
        &self,
        Parameters(args): Parameters<ReopenTaskArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "reopen_task",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                let cid = args
                    .target_column_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("target_column_id", s))
                    .transpose()?;
                self.app
                    .reopen_task(tid, cid, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Archive or restore a Kanban task.")]
    pub async fn archive_task(
        &self,
        Parameters(args): Parameters<ArchiveTaskArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "archive_task",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                self.app
                    .update_task(
                        tid,
                        UpdateTaskInput {
                            is_archived: Some(args.archived.unwrap_or(true)),
                            ..Default::default()
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Permanently delete a Kanban task (requires Destructive permission enabled)."
    )]
    pub async fn delete_task(&self, Parameters(args): Parameters<TaskIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "delete_task",
            AccessLevel::Destructive,
            None,
            args,
            |args, audit| {
                let tid = parse_uuid_arg("task_id", &args.task_id)?;
                self.app
                    .delete_task(tid, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List all overdue incomplete tasks for a profile.")]
    pub async fn get_overdue_tasks(
        &self,
        Parameters(args): Parameters<OverdueTasksArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_overdue_tasks",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app.get_overdue_tasks(pid).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List upcoming incomplete tasks due within the next N days (default 7).")]
    pub async fn get_upcoming_tasks(
        &self,
        Parameters(args): Parameters<UpcomingTasksArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_upcoming_tasks",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .get_upcoming_tasks(pid, args.within_days.unwrap_or(7))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List all active Kanban tasks linked to a specific subject.")]
    pub async fn get_tasks_by_subject(
        &self,
        Parameters(args): Parameters<TasksBySubjectArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_tasks_by_subject",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
                self.app
                    .list_tasks(pid, None, None, Some(sid), false)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Search tasks in a profile by title, description, notes, tags, or badges."
    )]
    pub async fn search_tasks(
        &self,
        Parameters(args): Parameters<SearchTasksArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "search_tasks",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .search_tasks(pid, &args.query)
                    .map_err(|e| e.to_string())
            },
        )
    }
}

impl OxidiumMcpServer {
    pub(crate) fn build_kanban_tasks_router()
    -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::kanban_tasks_router()
    }
}
