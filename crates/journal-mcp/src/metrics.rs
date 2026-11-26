use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    Stopped,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpMetricsSnapshot {
    pub status: McpServerStatus,
    pub transport: String,
    pub endpoint: String,
    pub server_version: String,
    pub protocol_compatibility: String,
    pub total_requests: u64,
    pub total_errors: u64,
    pub last_request_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

pub struct McpMetrics {
    status: Mutex<McpServerStatus>,
    transport: Mutex<String>,
    endpoint: Mutex<String>,
    last_request_at: Mutex<Option<DateTime<Utc>>>,
    last_error: Mutex<Option<String>>,
    total_requests: AtomicU64,
    total_errors: AtomicU64,
}

impl Default for McpMetrics {
    fn default() -> Self {
        Self {
            status: Mutex::new(McpServerStatus::Stopped),
            transport: Mutex::new("streamable_http".into()),
            endpoint: Mutex::new("http://127.0.0.1:38421/mcp".into()),
            last_request_at: Mutex::new(None),
            last_error: Mutex::new(None),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
        }
    }
}

impl McpMetrics {
    pub fn set_running(&self, transport: impl Into<String>, endpoint: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = McpServerStatus::Running;
        }
        if let Ok(mut t) = self.transport.lock() {
            *t = transport.into();
        }
        if let Ok(mut e) = self.endpoint.lock() {
            *e = endpoint.into();
        }
        if let Ok(mut err) = self.last_error.lock() {
            *err = None;
        }
    }

    pub fn set_stopped(&self) {
        if let Ok(mut s) = self.status.lock() {
            *s = McpServerStatus::Stopped;
        }
    }

    pub fn set_error(&self, message: impl Into<String>) {
        if let Ok(mut s) = self.status.lock() {
            *s = McpServerStatus::Error;
        }
        if let Ok(mut err) = self.last_error.lock() {
            *err = Some(message.into());
        }
    }

    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut ts) = self.last_request_at.lock() {
            *ts = Some(Utc::now());
        }
    }

    pub fn record_error(&self, msg: impl Into<String>) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut err) = self.last_error.lock() {
            *err = Some(msg.into());
        }
    }

    pub fn snapshot(&self) -> McpMetricsSnapshot {
        McpMetricsSnapshot {
            status: self
                .status
                .lock()
                .map(|s| *s)
                .unwrap_or(McpServerStatus::Stopped),
            transport: self.transport.lock().map(|t| t.clone()).unwrap_or_default(),
            endpoint: self.endpoint.lock().map(|e| e.clone()).unwrap_or_default(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_compatibility: "MCP 2025-03-26 / 2026-07-28 (rmcp 3.11)".into(),
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            last_request_at: self.last_request_at.lock().ok().and_then(|t| *t),
            last_error: self.last_error.lock().ok().and_then(|e| e.clone()),
        }
    }
}
