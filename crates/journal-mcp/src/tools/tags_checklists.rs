use journal_domain::RecordSource;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::permissions::{AccessLevel, parse_uuid_arg, run_tool};
use crate::server::OxidiumMcpServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProfileOnlyArgs {
    pub profile_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateTagOrBadgeArgs {
    pub profile_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateTagArgs {
    pub profile_id: String,
    pub tag_id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskTagArgs {
    pub task_id: String,
    pub tag_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskBadgeArgs {
    pub task_id: String,
    pub badge_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddChecklistItemArgs {
    pub task_id: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateChecklistItemArgs {
    pub item_id: String,
    pub title: Option<String>,
    pub is_completed: Option<bool>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChecklistItemIdArgs {
    pub item_id: String,
    pub completed: Option<bool>,
}

#[tool_router(router = tags_checklists_router)]
impl OxidiumMcpServer {
    #[tool(description = "List all tags in a profile.")]
    pub async fn list_tags(
        &self,
        Parameters(args): Parameters<ProfileOnlyArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_tags",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app.list_tags(pid).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Create a tag (returns existing tag case-insensitively to prevent duplicates).")]
    pub async fn create_tag(
        &self,
        Parameters(args): Parameters<CreateTagOrBadgeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "create_tag",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .get_or_create_tag(pid, &args.name, args.color.as_deref(), RecordSource::Mcp)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update a tag's name, color, or description.")]
    pub async fn update_tag(&self, Parameters(args): Parameters<UpdateTagArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_tag",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let tid = parse_uuid_arg("tag_id", &args.tag_id)?;
                self.app
                    .update_tag(
                        pid,
                        tid,
                        args.name,
                        args.color,
                        args.description,
                        RecordSource::Mcp,
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Assign an existing tag UUID to a task.")]
    pub async fn assign_tag(&self, Parameters(args): Parameters<TaskTagArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "assign_tag",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let task_id = parse_uuid_arg("task_id", &args.task_id)?;
                let tag_id = parse_uuid_arg("tag_id", &args.tag_id)?;
                self.app
                    .assign_tag(task_id, tag_id, RecordSource::Mcp)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "assigned": true, "task_id": task_id, "tag_id": tag_id }))
            },
        )
    }

    #[tool(description = "Remove a tag from a task.")]
    pub async fn remove_tag(&self, Parameters(args): Parameters<TaskTagArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "remove_tag",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let task_id = parse_uuid_arg("task_id", &args.task_id)?;
                let tag_id = parse_uuid_arg("tag_id", &args.tag_id)?;
                self.app
                    .remove_tag(task_id, tag_id, RecordSource::Mcp)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "removed": true, "task_id": task_id, "tag_id": tag_id }))
            },
        )
    }

    #[tool(description = "List all custom badges in a profile.")]
    pub async fn list_badges(
        &self,
        Parameters(args): Parameters<ProfileOnlyArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_badges",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app.list_badges(pid).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Create a custom badge (case-insensitive deduplication).")]
    pub async fn create_badge(
        &self,
        Parameters(args): Parameters<CreateTagOrBadgeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "create_badge",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .get_or_create_badge(pid, &args.name, args.color.as_deref(), RecordSource::Mcp)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Assign an existing badge UUID to a task.")]
    pub async fn assign_badge(
        &self,
        Parameters(args): Parameters<TaskBadgeArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "assign_badge",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let task_id = parse_uuid_arg("task_id", &args.task_id)?;
                let badge_id = parse_uuid_arg("badge_id", &args.badge_id)?;
                self.app
                    .assign_badge(task_id, badge_id, RecordSource::Mcp)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "assigned": true, "task_id": task_id, "badge_id": badge_id }))
            },
        )
    }

    #[tool(description = "Add a checklist item to a task.")]
    pub async fn add_checklist_item(
        &self,
        Parameters(args): Parameters<AddChecklistItemArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "add_checklist_item",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let task_id = parse_uuid_arg("task_id", &args.task_id)?;
                self.app
                    .add_checklist_item(task_id, args.title, RecordSource::Mcp)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update a checklist item's title, completion state, or sort order.")]
    pub async fn update_checklist_item(
        &self,
        Parameters(args): Parameters<UpdateChecklistItemArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_checklist_item",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("item_id", &args.item_id)?;
                self.app
                    .update_checklist_item(
                        id,
                        args.title,
                        args.is_completed,
                        args.sort_order,
                        RecordSource::Mcp,
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Mark a checklist item as completed (or uncompleted if completed=false).")]
    pub async fn complete_checklist_item(
        &self,
        Parameters(args): Parameters<ChecklistItemIdArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "complete_checklist_item",
            AccessLevel::Write,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("item_id", &args.item_id)?;
                self.app
                    .update_checklist_item(
                        id,
                        None,
                        Some(args.completed.unwrap_or(true)),
                        None,
                        RecordSource::Mcp,
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Delete a checklist item (requires Destructive permission enabled).")]
    pub async fn delete_checklist_item(
        &self,
        Parameters(args): Parameters<ChecklistItemIdArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "delete_checklist_item",
            AccessLevel::Destructive,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("item_id", &args.item_id)?;
                self.app
                    .delete_checklist_item(id, RecordSource::Mcp)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "deleted": true, "item_id": id }))
            },
        )
    }
}

impl OxidiumMcpServer {
    pub(crate) fn build_tags_checklists_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::tags_checklists_router()
    }
}
