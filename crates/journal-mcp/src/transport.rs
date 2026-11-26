use anyhow::{Context, Result};
use axum::{
    Router,
    extract::Request,
    http::{StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use journal_app::AppServices;
use rmcp::{
    ServiceExt,
    transport::{
        stdio,
        streamable_http_server::{
            StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
        },
    },
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::metrics::McpMetrics;
use crate::server::OxidiumMcpServer;

/// Runs the MCP server over STDIO (JSON-RPC on stdout; diagnostics strictly on stderr).
pub async fn run_stdio_server(app: Arc<AppServices>) -> Result<()> {
    let metrics = Arc::new(McpMetrics::default());
    metrics.set_running("stdio", "stdin/stdout");
    let server = OxidiumMcpServer::new(app, Arc::clone(&metrics));
    let running = server
        .serve(stdio())
        .await
        .context("failed to initialize MCP STDIO server")?;
    running
        .waiting()
        .await
        .context("MCP STDIO server terminated with error")?;
    metrics.set_stopped();
    Ok(())
}

/// Builds the authenticated Axum router for the loopback Streamable HTTP MCP server.
pub fn build_http_router(
    app: Arc<AppServices>,
    metrics: Arc<McpMetrics>,
    port: u16,
    ct: CancellationToken,
) -> Router {
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts([
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
            format!("localhost:{port}"),
            format!("127.0.0.1:{port}"),
        ])
        .with_json_response(true)
        .with_max_request_body_bytes(2 * 1024 * 1024)
        .with_cancellation_token(ct);

    let factory_app = Arc::clone(&app);
    let factory_metrics = Arc::clone(&metrics);
    let http_service: StreamableHttpService<OxidiumMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                Ok(OxidiumMcpServer::new(
                    Arc::clone(&factory_app),
                    Arc::clone(&factory_metrics),
                ))
            },
            Arc::new(LocalSessionManager::default()),
            config,
        );

    let auth_app = Arc::clone(&app);
    let auth_metrics = Arc::clone(&metrics);

    Router::new()
        .nest_service("/mcp", http_service)
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let app = Arc::clone(&auth_app);
            let metrics = Arc::clone(&auth_metrics);
            async move { bearer_auth_middleware(app, metrics, req, next).await }
        }))
}

async fn bearer_auth_middleware(
    app: Arc<AppServices>,
    metrics: Arc<McpMetrics>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let authorized = auth_header
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|token| app.tokens.verify_bearer(token));

    if !authorized {
        metrics.record_error("unauthorized HTTP MCP request (missing or invalid Bearer token)");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

/// Runs the loopback-only (`127.0.0.1:<port>`) Streamable HTTP MCP server until `ct` is cancelled.
pub async fn run_http_server(
    app: Arc<AppServices>,
    metrics: Arc<McpMetrics>,
    port: u16,
    ct: CancellationToken,
) -> Result<SocketAddr> {
    // Ensure a Bearer token exists before binding
    app.tokens
        .load_or_create()
        .context("failed to initialize MCP Bearer token")?;

    let bind_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind loopback MCP HTTP server on {bind_addr}"))?;
    let actual_addr = listener.local_addr()?;
    let endpoint = format!("http://{actual_addr}/mcp");

    let router = build_http_router(app, Arc::clone(&metrics), actual_addr.port(), ct.clone());
    metrics.set_running("streamable_http", endpoint.clone());
    tracing::info!(endpoint = %endpoint, "Oxidium Streamable HTTP MCP server listening (loopback only)");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            ct.cancelled().await;
        })
        .await
        .context("Streamable HTTP server error")?;

    metrics.set_stopped();
    Ok(actual_addr)
}

/// Manages the lifecycle of the embedded Streamable HTTP MCP server owned by the GUI process.
/// Clearly separated from external STDIO processes spawned by AI clients.
pub struct EmbeddedHttpManager {
    app: Arc<AppServices>,
    pub metrics: Arc<McpMetrics>,
    state: Mutex<Option<CancellationToken>>,
}

impl EmbeddedHttpManager {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self {
            app,
            metrics: Arc::new(McpMetrics::default()),
            state: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.state.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn start(&self, rt: &tokio::runtime::Handle, port: u16) -> Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("MCP manager lock poisoned"))?;
        if guard.is_some() {
            return Ok(());
        }
        let ct = CancellationToken::new();
        *guard = Some(ct.clone());
        drop(guard);

        let app = Arc::clone(&self.app);
        let metrics = Arc::clone(&self.metrics);
        let ct_for_task = ct.clone();
        rt.spawn(async move {
            if let Err(e) = run_http_server(app, Arc::clone(&metrics), port, ct_for_task).await {
                tracing::error!(error = %e, "embedded MCP HTTP server stopped with error");
                metrics.set_error(e.to_string());
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        if let Ok(mut guard) = self.state.lock()
            && let Some(ct) = guard.take()
        {
            ct.cancel();
            self.metrics.set_stopped();
        }
    }

    pub fn restart(&self, rt: &tokio::runtime::Handle, port: u16) -> Result<()> {
        self.stop();
        self.start(rt, port)
    }
}

impl Drop for EmbeddedHttpManager {
    fn drop(&mut self) {
        self.stop();
    }
}
