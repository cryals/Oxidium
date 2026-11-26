use anyhow::Result;
use journal_app::AppServices;
use journal_mcp::{McpMetrics, run_http_server, run_stdio_server};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn parse_args() -> (Option<PathBuf>, bool, Option<u16>, bool) {
    let mut args = std::env::args().skip(1);
    let mut db_path: Option<PathBuf> = None;
    let mut http_mode = false;
    let mut port: Option<u16> = None;
    let mut show_help = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                if let Some(p) = args.next() {
                    db_path = Some(PathBuf::from(p));
                }
            }
            "--http" => http_mode = true,
            "--stdio" => http_mode = false,
            "--port" => {
                if let Some(p) = args.next() {
                    port = p.parse().ok();
                }
            }
            "-h" | "--help" => show_help = true,
            _ => {}
        }
    }
    (db_path, http_mode, port, show_help)
}

#[tokio::main]
async fn main() -> Result<()> {
    // CRITICAL: Always write logs strictly to stderr so stdout remains clean JSON-RPC in STDIO mode.
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,journal_mcp=info")),
        )
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(false))
        .init();

    let (db_arg, http_mode, port_arg, show_help) = parse_args();
    if show_help {
        eprintln!(
            "Oxidium Student Journal MCP Server v{}\n\nUsage:\n  student-journal-mcp [--stdio | --http --port <PORT>] [--db <PATH>]\n\nDefaults to STDIO transport using the standard local Oxidium SQLite database.",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }

    let db_path = db_arg.unwrap_or_else(AppServices::default_db_path);
    let app = AppServices::open_file(&db_path)?;

    if http_mode {
        let settings = app.get_settings()?;
        let port = port_arg.unwrap_or(settings.mcp_http_port);
        let token = app.tokens.load_or_create()?;
        let metrics = Arc::new(McpMetrics::default());
        let ct = CancellationToken::new();
        let ct_sig = ct.clone();
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct_sig.cancel();
        });
        eprintln!(
            "Oxidium MCP Streamable HTTP starting on http://127.0.0.1:{port}/mcp (token file: {}, preview: {}...)",
            app.tokens.secret_path().display(),
            &token[..8.min(token.len())]
        );
        run_http_server(app, metrics, port, ct).await?;
    } else {
        run_stdio_server(app).await?;
    }

    Ok(())
}
