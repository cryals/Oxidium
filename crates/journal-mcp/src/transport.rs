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
use std::time::{Duration, Instant};
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

/// Validates optional HTTP `Origin` headers strictly against loopback origins (`localhost`,
/// `127.0.0.1`, `[::1]`) as required by the MCP Streamable HTTP security specification.
pub fn is_allowed_loopback_origin(origin: &str) -> bool {
    let trimmed = origin.trim();
    for prefix in ["http://127.0.0.1", "http://localhost", "http://[::1]"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if rest.is_empty() {
                return true;
            }
            if let Some(port_part) = rest.strip_prefix(':') {
                return !port_part.is_empty() && port_part.bytes().all(|b| b.is_ascii_digit());
            }
        }
    }
    false
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
    if let Some(origin_hdr) = req.headers().get(header::ORIGIN) {
        let valid_origin = origin_hdr
            .to_str()
            .ok()
            .is_some_and(is_allowed_loopback_origin);
        if !valid_origin {
            metrics.record_error("rejected HTTP MCP request due to non-loopback Origin header");
            return Err(StatusCode::FORBIDDEN);
        }
    }

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

async fn serve_bound_listener(
    app: Arc<AppServices>,
    metrics: Arc<McpMetrics>,
    listener: TcpListener,
    ct: CancellationToken,
) -> Result<SocketAddr> {
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

/// Runs the loopback-only (`127.0.0.1:<port>`) Streamable HTTP MCP server until `ct` is cancelled.
pub async fn run_http_server(
    app: Arc<AppServices>,
    metrics: Arc<McpMetrics>,
    port: u16,
    ct: CancellationToken,
) -> Result<SocketAddr> {
    app.tokens
        .load_or_create()
        .context("failed to initialize MCP Bearer token")?;

    let bind_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind loopback MCP HTTP server on {bind_addr}"))?;
    serve_bound_listener(app, metrics, listener, ct).await
}

type EmbeddedTaskState = Option<(CancellationToken, tokio::task::JoinHandle<()>)>;

/// Manages the lifecycle of the embedded Streamable HTTP MCP server owned by the GUI process.
/// Clearly separated from external STDIO processes spawned by AI clients.
pub struct EmbeddedHttpManager {
    app: Arc<AppServices>,
    pub metrics: Arc<McpMetrics>,
    state: Arc<Mutex<EmbeddedTaskState>>,
}

impl EmbeddedHttpManager {
    pub fn new(app: Arc<AppServices>) -> Self {
        Self {
            app,
            metrics: Arc::new(McpMetrics::default()),
            state: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.state
            .lock()
            .map(|g| g.as_ref().is_some_and(|(_, h)| !h.is_finished()))
            .unwrap_or(false)
    }

    /// Synchronously validates Bearer token creation and binds `127.0.0.1:<port>` before returning,
    /// so bind errors surface immediately and `metrics` reflects `Running` state right away.
    pub fn start(&self, rt: &tokio::runtime::Handle, port: u16) -> Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("MCP manager lock poisoned"))?;
        if let Some((ct, handle)) = guard.as_ref() {
            if !handle.is_finished() && !ct.is_cancelled() {
                return Ok(());
            }
            guard.take();
        }

        self.app
            .tokens
            .load_or_create()
            .context("failed to initialize MCP Bearer token")?;

        let bind_addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let std_listener = std::net::TcpListener::bind(bind_addr)
            .with_context(|| format!("failed to bind loopback MCP HTTP server on {bind_addr}"))?;
        std_listener.set_nonblocking(true)?;
        let actual_addr = std_listener.local_addr()?;
        let endpoint = format!("http://{actual_addr}/mcp");

        let listener = {
            let _enter = rt.enter();
            TcpListener::from_std(std_listener)?
        };

        let ct = CancellationToken::new();
        self.metrics.set_running("streamable_http", endpoint);

        let app = Arc::clone(&self.app);
        let metrics = Arc::clone(&self.metrics);
        let state_arc = Arc::clone(&self.state);
        let ct_for_task = ct.clone();
        let handle = rt.spawn(async move {
            if let Err(e) =
                serve_bound_listener(app, Arc::clone(&metrics), listener, ct_for_task).await
            {
                tracing::error!(error = %e, "embedded MCP HTTP server stopped with error");
                metrics.set_error(e.to_string());
            }
            if let Ok(mut g) = state_arc.lock() {
                *g = None;
            }
        });

        *guard = Some((ct, handle));
        Ok(())
    }

    fn take_and_cancel(&self) -> Option<tokio::task::JoinHandle<()>> {
        let mut guard = self.state.lock().ok()?;
        let (ct, handle) = guard.take()?;
        ct.cancel();
        self.metrics.set_stopped();
        Some(handle)
    }

    pub fn stop(&self) {
        let _ = self.take_and_cancel();
    }

    pub fn restart(&self, rt: &tokio::runtime::Handle, port: u16) -> Result<()> {
        if let Some(handle) = self.take_and_cancel() {
            let deadline = Instant::now() + Duration::from_millis(600);
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.start(rt, port)
    }
}

impl Drop for EmbeddedHttpManager {
    fn drop(&mut self) {
        self.stop();
    }
}
