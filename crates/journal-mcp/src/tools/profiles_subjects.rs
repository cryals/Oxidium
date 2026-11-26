use journal_app::{CreateProfileInput, CreateSubjectInput, UpdateProfileInput, UpdateSubjectInput};
use journal_domain::RecordSource;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::permissions::{AccessLevel, parse_uuid_arg, run_tool};
use crate::server::OxidiumMcpServer;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct ListProfilesArgs {
    #[schemars(description = "Include archived profiles (default false)")]
    pub include_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProfileIdArgs {
    #[schemars(description = "Profile UUID")]
    pub profile_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateProfileArgs {
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub institution: Option<String>,
    pub group_name: Option<String>,
    pub academic_year: Option<String>,
    pub description: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateProfileArgs {
    pub profile_id: String,
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub institution: Option<String>,
    pub group_name: Option<String>,
    pub academic_year: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveProfileArgs {
    pub profile_id: String,
    #[schemars(description = "True to archive, false to restore (default true)")]
    pub archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListSubjectsArgs {
    pub profile_id: String,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SubjectIdArgs {
    pub subject_id: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FindSubjectArgs {
    pub profile_id: String,
    #[schemars(description = "Subject full name or short abbreviation to search")]
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateSubjectArgs {
    pub profile_id: String,
    pub name: String,
    pub short_name: Option<String>,
    pub teacher: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub academic_period_id: Option<String>,
    pub target_average: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateSubjectArgs {
    pub subject_id: String,
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub teacher: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub target_average: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveSubjectArgs {
    pub subject_id: String,
    pub archived: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ReorderSubjectsArgs {
    pub profile_id: String,
    pub ordered_subject_ids: Vec<String>,
}

#[tool_router(router = profiles_subjects_router)]
impl OxidiumMcpServer {
    #[tool(description = "List all academic profiles (active by default, optionally archived).")]
    pub async fn list_profiles(
        &self,
        Parameters(args): Parameters<ListProfilesArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_profiles",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                self.app
                    .list_profiles(args.include_archived.unwrap_or(false))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a single academic profile by UUID.")]
    pub async fn get_profile(&self, Parameters(args): Parameters<ProfileIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_profile",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app.get_profile(id).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get the currently active academic profile.")]
    pub async fn get_active_profile(&self) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_active_profile",
            AccessLevel::Read,
            None,
            (),
            |_, _| self.app.get_active_profile().map_err(|e| e.to_string()),
        )
    }

    #[tool(description = "Create a new isolated academic profile (with default Kanban board).")]
    pub async fn create_profile(
        &self,
        Parameters(args): Parameters<CreateProfileArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_profile",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                self.app
                    .create_profile(
                        CreateProfileInput {
                            name: args.name,
                            icon: args.icon,
                            color: args.color,
                            institution: args.institution,
                            group_name: args.group_name,
                            academic_year: args.academic_year,
                            description: args.description,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update an existing academic profile's metadata.")]
    pub async fn update_profile(
        &self,
        Parameters(args): Parameters<UpdateProfileArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_profile",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .update_profile(
                        id,
                        UpdateProfileInput {
                            name: args.name,
                            icon: args.icon,
                            color: args.color,
                            institution: args.institution,
                            group_name: args.group_name,
                            academic_year: args.academic_year,
                            description: args.description,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Archive or restore an academic profile without deleting its data.")]
    pub async fn archive_profile(
        &self,
        Parameters(args): Parameters<ArchiveProfileArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "archive_profile",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let id = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .set_profile_archived(
                        id,
                        args.archived.unwrap_or(true),
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "List all subjects in a specific academic profile.")]
    pub async fn list_subjects(
        &self,
        Parameters(args): Parameters<ListSubjectsArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "list_subjects",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .list_subjects(pid, args.include_archived.unwrap_or(false))
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Get a single subject by UUID.")]
    pub async fn get_subject(&self, Parameters(args): Parameters<SubjectIdArgs>) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "get_subject",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let id = parse_uuid_arg("subject_id", &args.subject_id)?;
                self.app.get_subject(id).map_err(|e| e.to_string())
            },
        )
    }

    #[tool(
        description = "Find a subject by name or abbreviation within a profile. Returns exact match or candidate list when ambiguous."
    )]
    pub async fn find_subject(
        &self,
        Parameters(args): Parameters<FindSubjectArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "find_subject",
            AccessLevel::Read,
            None,
            args,
            |args, _| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                self.app
                    .find_subject(pid, &args.query)
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Create a new subject in the explicitly specified profile.")]
    pub async fn create_subject(
        &self,
        Parameters(args): Parameters<CreateSubjectArgs>,
    ) -> CallToolResult {
        let idem = args.idempotency_key.clone();
        run_tool(
            &self.app,
            &self.metrics,
            "create_subject",
            AccessLevel::Write,
            idem.as_deref(),
            args,
            |args, audit| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let ap = args
                    .academic_period_id
                    .as_deref()
                    .map(|s| parse_uuid_arg("academic_period_id", s))
                    .transpose()?;
                let tgt = args
                    .target_average
                    .as_deref()
                    .map(|s| {
                        Decimal::from_str(s)
                            .map_err(|_| format!("invalid target_average decimal: '{s}'"))
                    })
                    .transpose()?;
                self.app
                    .create_subject(
                        CreateSubjectInput {
                            profile_id: pid,
                            name: args.name,
                            short_name: args.short_name,
                            teacher: args.teacher,
                            color: args.color,
                            icon: args.icon,
                            description: args.description,
                            academic_period_id: ap,
                            target_average: tgt,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Update an existing subject's properties.")]
    pub async fn update_subject(
        &self,
        Parameters(args): Parameters<UpdateSubjectArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "update_subject",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
                let tgt = match args.target_average {
                    Some(ref s) if s.trim().is_empty() => Some(None),
                    Some(ref s) => {
                        Some(Some(Decimal::from_str(s.trim()).map_err(|_| {
                            format!("invalid target_average decimal: '{s}'")
                        })?))
                    }
                    None => None,
                };
                self.app
                    .update_subject(
                        sid,
                        UpdateSubjectInput {
                            name: args.name,
                            short_name: args.short_name,
                            teacher: args.teacher,
                            color: args.color,
                            icon: args.icon,
                            description: args.description,
                            academic_period_id: None,
                            target_average: tgt,
                        },
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Archive or restore a subject (preserves historical grades).")]
    pub async fn archive_subject(
        &self,
        Parameters(args): Parameters<ArchiveSubjectArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "archive_subject",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let sid = parse_uuid_arg("subject_id", &args.subject_id)?;
                self.app
                    .set_subject_archived(
                        sid,
                        args.archived.unwrap_or(true),
                        RecordSource::Mcp,
                        Some(audit),
                    )
                    .map_err(|e| e.to_string())
            },
        )
    }

    #[tool(description = "Reorder subjects within a profile by providing ordered subject UUIDs.")]
    pub async fn reorder_subjects(
        &self,
        Parameters(args): Parameters<ReorderSubjectsArgs>,
    ) -> CallToolResult {
        run_tool(
            &self.app,
            &self.metrics,
            "reorder_subjects",
            AccessLevel::Write,
            None,
            args,
            |args, audit| {
                let pid = parse_uuid_arg("profile_id", &args.profile_id)?;
                let mut ids = Vec::with_capacity(args.ordered_subject_ids.len());
                for raw in &args.ordered_subject_ids {
                    ids.push(parse_uuid_arg("ordered_subject_ids[]", raw)?);
                }
                self.app
                    .reorder_subjects(pid, &ids, RecordSource::Mcp, Some(audit))
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({ "reordered": ids.len() }))
            },
        )
    }
}

impl OxidiumMcpServer {
    pub(crate) fn build_profiles_subjects_router()
    -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::profiles_subjects_router()
    }
}
