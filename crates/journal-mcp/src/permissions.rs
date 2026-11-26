use journal_app::{AppServices, AuditMeta};
use journal_domain::McpPermissionMode;
use journal_storage::IdempotencyRecord;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::metrics::McpMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    Read,
    Write,
    Destructive,
}

/// Enforces server-side Rust MCP permissions based on live persisted settings.
pub fn check_permission(
    app: &AppServices,
    level: AccessLevel,
    tool_name: &str,
) -> Result<(), String> {
    let settings = app
        .get_settings()
        .map_err(|e| format!("failed to load MCP permission settings: {e}"))?;
    match level {
        AccessLevel::Read => Ok(()),
        AccessLevel::Write => match settings.mcp_permission_mode {
            McpPermissionMode::ReadOnly => Err(format!(
                "Permission denied: tool '{tool_name}' requires Read+Write mode, but MCP server is currently in Read-Only mode. Enable Read+Write in Oxidium Settings → MCP."
            )),
            McpPermissionMode::ReadWrite => Ok(()),
        },
        AccessLevel::Destructive => {
            if settings.mcp_permission_mode == McpPermissionMode::ReadOnly {
                return Err(format!(
                    "Permission denied: destructive tool '{tool_name}' is blocked in Read-Only mode."
                ));
            }
            if !settings.mcp_allow_destructive {
                return Err(format!(
                    "Permission denied: destructive tool '{tool_name}' requires enabling Destructive Actions in Oxidium Settings → MCP."
                ));
            }
            Ok(())
        }
    }
}

pub fn err_result(metrics: &McpMetrics, msg: impl Into<String>) -> CallToolResult {
    let m = msg.into();
    metrics.record_error(&m);
    CallToolResult::error(vec![ContentBlock::text(m)])
}

/// Wraps a tool invocation with metrics, permission gate, and optional idempotency deduplication.
pub fn run_tool<P, R, F>(
    app: &AppServices,
    metrics: &McpMetrics,
    tool_name: &str,
    level: AccessLevel,
    idempotency_key: Option<&str>,
    args: P,
    f: F,
) -> CallToolResult
where
    P: Serialize,
    R: Serialize,
    F: FnOnce(P, &AuditMeta) -> Result<R, String>,
{
    metrics.record_request();
    if let Err(denied) = check_permission(app, level, tool_name) {
        return err_result(metrics, denied);
    }

    let req_hash = serde_json::to_vec(&args)
        .map(|b| hex::encode(Sha256::digest(&b)))
        .unwrap_or_default();

    if let Some(key) = idempotency_key
        && !key.trim().is_empty()
        && let Ok(Some(existing)) = app.db.get_idempotency(key.trim())
    {
        if existing.tool_name != tool_name || existing.request_hash != req_hash {
            return err_result(
                metrics,
                format!(
                    "idempotency_key '{key}' was already used with different tool or parameters"
                ),
            );
        }
        return CallToolResult::success(vec![ContentBlock::text(existing.response_json)]);
    }

    let audit = AuditMeta::mcp(Some("MCP Client".into()), tool_name);
    match f(args, &audit) {
        Ok(value) => {
            let json = match serde_json::to_string_pretty(&value) {
                Ok(s) => s,
                Err(e) => return err_result(metrics, format!("serialization error: {e}")),
            };
            if let Some(key) = idempotency_key
                && !key.trim().is_empty()
            {
                let _ = app.db.save_idempotency(&IdempotencyRecord {
                    key: key.trim().to_string(),
                    tool_name: tool_name.to_string(),
                    request_hash: req_hash,
                    response_json: json.clone(),
                });
            }
            CallToolResult::success(vec![ContentBlock::text(json)])
        }
        Err(msg) => err_result(metrics, msg),
    }
}

pub fn parse_uuid_arg(field: &str, value: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(value.trim())
        .map_err(|_| format!("invalid UUID for '{field}': '{value}'"))
}

pub fn parse_date_arg(field: &str, value: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| format!("invalid ISO 8601 date (YYYY-MM-DD) for '{field}': '{value}'"))
}

pub fn parse_dt_arg(field: &str, value: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value.trim()) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        && let Some(naive_dt) = d.and_hms_opt(23, 59, 0)
    {
        return Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            naive_dt,
            chrono::Utc,
        ));
    }
    Err(format!(
        "invalid ISO 8601 datetime or date for '{field}': '{value}'"
    ))
}
